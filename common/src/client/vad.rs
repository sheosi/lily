use std::path::Path;
use thiserror::Error;

pub trait Vad {
    fn reset(&mut self) -> Result<(), VadError>;
    fn is_someone_talking(&mut self, audio: &[i16]) -> Result<bool, VadError>;
}

#[cfg(feature = "snowboy")]
pub struct SnowboyVad {
    vad: rsnowboy::SnowboyVad,
}

#[cfg(feature = "snowboy")]
impl SnowboyVad {
    pub fn new(res_path: &Path) -> Result<Self, VadError> {
        let vad = rsnowboy::SnowboyVad::new(res_path.to_str().ok_or(VadError::NotUnicode)?);
        Ok(Self { vad })
    }
}

#[cfg(feature = "snowboy")]
impl Vad for SnowboyVad {
    fn reset(&mut self) -> Result<(), VadError> {
        self.vad.reset();
        Ok(())
    }

    fn is_someone_talking(&mut self, audio: &[i16]) -> Result<bool, VadError> {
        let vad_val = self
            .vad
            .run_short_array(&audio[0] as *const i16, audio.len() as i32, false);
        if vad_val == -1 {
            // Maybe whe should do something worse with this is (return a result)
            log::error!("Something happened in the Vad");
            Err(VadError::Unknown)
        } else {
            Ok(vad_val == 0)
        }
    }
}

#[cfg(feature = "webrtc_vad")]
pub struct WebRtcVad {
    vad: webrtc_vad::Vad,
}

#[cfg(feature = "webrtc_vad")]
impl WebRtcVad {
    pub fn new() -> Self {
        Self {
            vad: webrtc_vad::Vad::new_with_rate(webrtc_vad::SampleRate::Rate16kHz),
        }
    }
}

#[cfg(feature = "webrtc_vad")]
impl Vad for WebRtcVad {
    fn reset(&mut self) -> Result<(), VadError> {
        self.vad.reset();
        Ok(())
    }

    fn is_someone_talking(&mut self, audio: &[i16]) -> Result<bool, VadError> {
        self.vad
            .is_voice_segment(audio)
            .map_err(|_| VadError::InvalidFrameLength)
    }
}

#[derive(Error, Debug)]
pub enum VadError {
    #[error("Something happened in the Vad")]
    Unknown,

    #[error("Input was not unicode")]
    NotUnicode,

    #[error("Invalid frame length")]
    InvalidFrameLength,
}
