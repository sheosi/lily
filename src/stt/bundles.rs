use crate::audio::AudioRaw;
use crate::stt::{DecodeState, SttBatched, SttError, SttStream, SttVadless, SttInfo};
use crate::vad::Vad;
use crate::vars::DEFAULT_SAMPLES_PER_SECOND;


pub struct SttBatcher<S: SttBatched, V: Vad> {
    batch_stt: S,
    vad: V,
    copy_audio: AudioRaw,   
    someone_was_talking: bool
}

impl<S: SttBatched, V: Vad> SttBatcher<S, V> {
    pub fn new(batch_stt: S, vad: V) -> Self {
        Self {vad, copy_audio: AudioRaw::new_empty(DEFAULT_SAMPLES_PER_SECOND), batch_stt, someone_was_talking: false}
    }
}


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


pub struct SttOnlineInterface<S: SttBatched, F: SttStream> {
    online_stt: S,
    fallback: F,
    copy_audio: AudioRaw,
}

impl<S: SttBatched, F: SttStream> SttOnlineInterface<S, F> {
    pub fn new(online_stt: S,fallback: F) -> Self {
        Self{online_stt, fallback, copy_audio: AudioRaw::new_empty(DEFAULT_SAMPLES_PER_SECOND)}
    }
}

impl<S: SttBatched, F: SttStream> SttStream for SttOnlineInterface<S, F> {
    fn begin_decoding(&mut self) -> Result<(),SttError> {
        self.copy_audio.clear();
        self.fallback.begin_decoding()?;
        Ok(())

    }

    fn decode(&mut self, audio: &[i16]) -> Result<DecodeState, SttError> {
        let res = self.fallback.decode(audio)?;
        if res != DecodeState::NotStarted {
            self.copy_audio.append_audio(audio, DEFAULT_SAMPLES_PER_SECOND);
        }
        match res {
            DecodeState::Finished(local_res) => {
                match self.online_stt.decode(&self.copy_audio.buffer) {
                    Ok(ok_res) => Ok(DecodeState::Finished(ok_res)),
                    Err(_) => Ok(DecodeState::Finished(local_res))
                }

            },
            _ => Ok(res)
        }
    }

    fn get_info(&self) -> SttInfo {
        self.online_stt.get_info()
    }

}

pub struct SttVadlessInterface<S: SttVadless, F: SttStream> {
    online_stt: S,
    fallback: F,
}


impl<S: SttVadless, F: SttStream> SttVadlessInterface<S, F> {
    pub fn new(online_stt: S,fallback: F) -> Self {
        Self{online_stt, fallback}
    }
}

impl<S: SttVadless, F: SttStream> SttStream for SttVadlessInterface<S,F> {
    fn begin_decoding(&mut self) -> Result<(),SttError> {
        self.fallback.begin_decoding()?;
        Ok(())

    }

    fn decode(&mut self, audio: &[i16]) -> Result<DecodeState, SttError> {
        self.online_stt.process(audio)?;
        let res = self.fallback.decode(audio)?;
        match res {
            DecodeState::Finished(local_res) => {
                match self.online_stt.end_decoding() {
                    Ok(ok_res) => Ok(DecodeState::Finished(ok_res)),
                    Err(_) => Ok(DecodeState::Finished(local_res))
                }

            },
            _ => Ok(res)
        }
    }

    fn get_info(&self) -> SttInfo {
        self.online_stt.get_info()
    }
}