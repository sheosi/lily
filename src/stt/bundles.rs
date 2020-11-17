
use crate::stt::{DecodeRes, SttError, Stt, SttInfo};
use crate::vars::DEFAULT_SAMPLES_PER_SECOND;

use async_trait::async_trait;
use lily_common::audio::AudioRaw;

#[cfg(feature = "unused")]
use crate::stt::SttBatched;

use log::warn;

#[cfg(feature = "unused")]
pub struct SttBatcher<S: SttBatched> {
    batch_stt: S,
    copy_audio: AudioRaw,   
    someone_was_talking: bool
}

#[cfg(feature = "unused")]
impl<S: SttBatched> SttBatcher<S> {
    pub fn new(batch_stt: S) -> Self {
        Self {copy_audio: AudioRaw::new_empty(DEFAULT_SAMPLES_PER_SECOND), batch_stt, someone_was_talking: false}
    }
}

#[cfg(feature = "unused")]
#[async_trait(?Send)]
impl<S: SttBatched> Stt for SttBatcher<S> {
    async fn begin_decoding(&mut self) -> Result<(),SttError> {
        self.copy_audio.clear();
        self.someone_was_talking = false;

        Ok(())
    }

    async fn process(&mut self, audio: &[i16]) -> Result<(), SttError> {
        self.copy_audio.append_audio(audio, DEFAULT_SAMPLES_PER_SECOND);
        Ok(())
    }

    async fn end_decoding(&mut self) -> Result<Option<DecodeRes>, SttError> {
        self.batch_stt.decode(&self.copy_audio.buffer).await
    }

    fn get_info(&self) -> SttInfo {
        self.batch_stt.get_info()
    }

}


pub struct SttFallback<S: Stt> {
    main_stt: S,
    fallback: Box<dyn Stt>,
    copy_audio: AudioRaw,
    using_fallback: bool
}

impl<S: Stt> SttFallback<S> {
    pub fn new(main_stt: S,fallback: Box<dyn Stt>) -> Self {
        Self{main_stt, fallback,
            copy_audio: AudioRaw::new_empty(DEFAULT_SAMPLES_PER_SECOND), 
            using_fallback: false
        }
    }
}

#[async_trait(?Send)]
impl<S: Stt> Stt for SttFallback<S> {
    async fn begin_decoding(&mut self) -> Result<(),SttError> {
        self.copy_audio.clear();
        self.main_stt.begin_decoding().await?;
        Ok(())
    }

    async fn process(&mut self, audio: &[i16]) -> Result<(), SttError> {
        if !self.using_fallback {
            match self.main_stt.process(audio).await {
                Ok(()) => {
                    self.copy_audio.append_audio(audio, DEFAULT_SAMPLES_PER_SECOND);
                    Ok(())
                },
                Err(err) => {
                    warn!("Problem with online STT: {}", err);
                    self.fallback.begin_decoding().await?;
                    self.copy_audio.append_audio(audio, DEFAULT_SAMPLES_PER_SECOND);
                    let inter_res = self.fallback.process(&self.copy_audio.buffer).await;
                    self.copy_audio.clear(); // We don't need the copy audio anymore
                    self.using_fallback = true;

                    inter_res
                }
                
            }
        }
        else {
            self.fallback.process(audio).await
        }
    }

    async fn end_decoding(&mut self) -> Result<Option<DecodeRes>, SttError> {
        let res = if !self.using_fallback {
            let res = match self.main_stt.end_decoding().await {
                Ok(res) => Ok(res),
                Err(err) => {
                    warn!("Problem with online STT: {}", err);
                    self.fallback.begin_decoding().await?;
                    self.fallback.end_decoding().await
                }
                
            };
            self.copy_audio.clear(); // We don't wathever is in here anymore
            res
        }
        else {
            self.using_fallback = false; // Clear this flag
            self.fallback.end_decoding().await
        };
        res
    }


    fn get_info(&self) -> SttInfo {
        self.main_stt.get_info()
    }

}

