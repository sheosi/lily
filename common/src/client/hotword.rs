use std::path::Path;
use crate::client::vad::VadError;
use log::info;
use anyhow::{anyhow, Result};
use thiserror::Error;

#[cfg(feature="unused")]
use crate::client::vad::Vad;

pub trait HotwordDetector {
    fn start_hotword_check(&mut self) -> Result<(), VadError>;
    fn check_hotword(&mut self, audio: &[i16]) -> Result<bool>;
    fn set_sensitivity(&mut self, value: f32);
}

pub struct Snowboy {
    detector: rsnowboy::SnowboyDetect,
}

impl Snowboy {
    pub fn new(model_path: &Path, res_path: &Path, sensitivity: f32) -> Result<Snowboy> {

        let res_path_str = res_path.to_str().ok_or_else(||anyhow!("Failed to transform resource path to unicode {:?}", res_path))?;
        let model_path_str = model_path.to_str().ok_or_else(||anyhow!("Failed to transform model path to unicode {:?}", model_path))?;

        let detector = rsnowboy::SnowboyDetect::new(res_path_str, model_path_str);
        detector.set_sensitivity(sensitivity.to_string());
        detector.set_audio_gain(1.0);
        detector.apply_frontend(false);

        Ok(Snowboy {detector})
    }

    pub fn detector_check(&mut self, audio: &[i16]) -> i32 {
        self.detector.run_short_array_detection(&audio[0] as *const i16, audio.len() as i32, false)
    }
}

impl HotwordDetector for Snowboy {
    fn start_hotword_check(&mut self) -> Result<(), VadError> {
        self.detector.reset();
        info!("WaitingForHotword");

        Ok(())
    }

    fn check_hotword(&mut self, audio: &[i16]) -> Result<bool> {
        match self.detector_check(audio) {
            1      => Ok(true),
            0 | -2 => Ok(false),
            -1     => Err(HotwordError::Unknown.into()),
            _ => {panic!("Received from snowboy a wrong value")}
        }
    }

    fn set_sensitivity(&mut self, value: f32) {
        self.detector.set_sensitivity(value.to_string());
    }
}

#[derive(Error, Debug)]
pub enum HotwordError{
    #[error("Something happened with the hotword engine")]
    Unknown,

    #[error("Something happend with the vad engine")]
    VadError
}

impl std::convert::From<VadError> for HotwordError {
    fn from(_err: VadError) -> Self {
        HotwordError::VadError
    }
}

#[cfg(feature="unused")]
// Wrap a hotword engine with a vad to minimize resource consumption
struct VadHotword<V: Vad ,H: HotwordDetector> {
    vad: V,
    someone_talking: bool,
    hotword_eng: H

}

#[cfg(feature="unused")]
impl<V: Vad, H: HotwordDetector> VadHotword<V, H> {
    fn new(vad: V, hotword_eng: H) -> Self {
        Self {vad, someone_talking: false, hotword_eng}
    }
}

#[cfg(feature="unused")]
impl<V: Vad, H: HotwordDetector> HotwordDetector for VadHotword<V, H> {
    fn start_hotword_check(&mut self) -> Result<(), VadError> {
        self.hotword_eng.start_hotword_check()?;
        self.vad.reset()?;
        self.someone_talking = false;
        info!("WaitingForHotword");

        Ok(())
    }

    fn check_hotword(&mut self, audio: &[i16]) -> Result<bool> {
        let are_they_talking = self.vad.is_someone_talking(audio)?;

        if are_they_talking {
            self.someone_talking = true;
            let detector_res = self.hotword_eng.check_hotword(audio)?;
            Ok(detector_res)
        }
        else {
            if self.someone_talking {
                self.hotword_eng.start_hotword_check()?; // Restart if no one is talking anymore
            }
            self.someone_talking = false;
            Ok(false)
        }
    }

    fn set_sensitivity(&mut self, value: f32) {
        self.hotword_eng.set_sensitivity(value)
    }
}