use crate::audio::AudioRaw;
use crate::stt::{DecodeState, SttError, SttStream, SttVadless, SttInfo};
use crate::vad::Vad;
use crate::vars::DEFAULT_SAMPLES_PER_SECOND;

#[cfg(feature = "unused_stt_batcher")]
use crate::stt::SttBatched;

use log::warn;

#[cfg(feature = "unused_stt_batcher")]
pub struct SttBatcher<S: SttBatched, V: Vad> {
    batch_stt: S,
    vad: V,
    copy_audio: AudioRaw,   
    someone_was_talking: bool
}

#[cfg(feature = "unused_stt_batcher")]
impl<S: SttBatched, V: Vad> SttBatcher<S, V> {
    pub fn new(batch_stt: S, vad: V) -> Self {
        Self {vad, copy_audio: AudioRaw::new_empty(DEFAULT_SAMPLES_PER_SECOND), batch_stt, someone_was_talking: false}
    }
}

#[cfg(feature = "unused_stt_batcher")]
impl<S: SttBatched, V: Vad> SttStream for SttBatcher<S, V> {
    fn begin_decoding(&mut self) -> Result<(),SttError> {
        self.copy_audio.clear();
        self.vad.reset()?;
        self.someone_was_talking = false;

        Ok(())
    }

    fn decode(&mut self, audio: &[i16]) -> Result<DecodeState, SttError> {
        self.copy_audio.append_audio(audio, DEFAULT_SAMPLES_PER_SECOND);
        if self.vad.is_someone_talking(audio)? {
            if self.someone_was_talking {
                // We are still getting talke
                Ok(DecodeState::NotFinished)
            }
            else {
                self.someone_was_talking = true;
                Ok(DecodeState::StartListening)
            }
        }
        else {
            if self.someone_was_talking {
                let res = self.batch_stt.decode(&self.copy_audio.buffer)?;
                self.someone_was_talking = false;
                Ok(DecodeState::Finished(res))
            }
            else {
                Ok(DecodeState::NotStarted)
            }

        }
    }

    fn get_info(&self) -> SttInfo {
        self.batch_stt.get_info()
    }

}


pub struct SttFallback<S: SttStream> {
    main_stt: S,
    fallback: Box<dyn SttStream>,
    copy_audio: AudioRaw,
    using_fallback: bool
}

impl<S: SttStream> SttFallback<S> {
    pub fn new(main_stt: S,fallback: Box<dyn SttStream>) -> Self {
        Self{main_stt, fallback,
            copy_audio: AudioRaw::new_empty(DEFAULT_SAMPLES_PER_SECOND), 
            using_fallback: false
        }
    }
}

impl<S: SttStream> SttStream for SttFallback<S> {
    fn begin_decoding(&mut self) -> Result<(),SttError> {
        self.copy_audio.clear();
        self.main_stt.begin_decoding()?;
        Ok(())

    }

    fn decode(&mut self, audio: &[i16]) -> Result<DecodeState, SttError> {
        if !self.using_fallback {
            match self.main_stt.decode(audio) {
                Ok(inter_res) => {
                    if inter_res != DecodeState::NotStarted {
                        self.copy_audio.append_audio(audio, DEFAULT_SAMPLES_PER_SECOND);
                    }

                    Ok(inter_res)
                },
                Err(err) => {
                    warn!("Problem with online STT: {}", err);
                    self.fallback.begin_decoding()?;
                    self.copy_audio.append_audio(audio, DEFAULT_SAMPLES_PER_SECOND);
                    let inter_res = self.fallback.decode(&self.copy_audio.buffer);
                    self.copy_audio.clear(); // We don't need the copy audio anymore
                    self.using_fallback = true;

                    inter_res
                }
                
            }
        }
        else {
            self.fallback.decode(audio)
        }
    }

    fn get_info(&self) -> SttInfo {
        self.main_stt.get_info()
    }

}

pub struct SttVadlessInterface<S: SttVadless, V: Vad> {
    vadless: S,
    vad: V,
    someone_was_talking: bool
}


impl<S: SttVadless, V: Vad> SttVadlessInterface<S, V> {
    pub fn new(vadless: S, vad: V) -> Self {
        Self{vadless, vad, someone_was_talking: false}
    }
}

impl<S: SttVadless, V: Vad> SttStream for SttVadlessInterface<S,V> {
    fn begin_decoding(&mut self) -> Result<(),SttError> {
        self.vad.reset()?;
        self.vadless.begin_decoding()?;
        Ok(())

    }

    fn decode(&mut self, audio: &[i16]) -> Result<DecodeState, SttError> {
        self.vadless.process(audio)?;
        if self.vad.is_someone_talking(audio)? {
            if self.someone_was_talking {
                // We are still getting talke
                Ok(DecodeState::NotFinished)
            }
            else {
                self.someone_was_talking = true;
                Ok(DecodeState::StartListening)
            }
        }
        else {
            if self.someone_was_talking {
                let res = self.vadless.end_decoding()?;
                self.someone_was_talking = false;
                Ok(DecodeState::Finished(res))
            }
            else {
                Ok(DecodeState::NotStarted)
            }

        }
    }

    fn get_info(&self) -> SttInfo {
        self.vadless.get_info()
    }
}