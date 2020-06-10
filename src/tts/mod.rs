use core::fmt::Display;

mod error;
mod ibm;
mod pico;


pub use self::error::*;
pub use self::ibm::*;
pub use self::pico::*;

#[cfg(feature = "extra_langs_tts")]
mod espeak;
#[cfg(feature = "extra_langs_tts")]
pub use self::espeak::*;
#[cfg(feature = "google_tts")]
mod google;
#[cfg(feature = "google_tts")]
pub use self::google::*;

use crate::audio::Audio;

use unic_langid::LanguageIdentifier;


#[derive(Debug, Clone)]
pub struct TtsInfo {
    pub name: String,
    pub is_online: bool
}

impl Display for TtsInfo {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::result::Result<(), std::fmt::Error> {
        let online_str = {
            if self.is_online {"online"}
            else {"local"}

        };
        
        write!(formatter, "{}({})", self.name, online_str)
    }
}


#[derive(Debug, Clone, PartialEq)]
pub enum Gender {
    Male,
    Female
}
#[derive(Debug, Clone)]
pub struct VoiceDescr {
    pub gender: Gender
}

pub trait Tts {
    fn synth_text(&mut self, input: &str) -> Result<Audio, TtsError>;
    fn get_info(&self) -> TtsInfo;
}

pub trait TtsStatic {
    fn check_compatible(descr: &VoiceDescr) -> Result<(), TtsConstructionError>;
}

pub struct DummyTts{}

impl DummyTts {
    pub fn new() -> Self{
        Self{}
    }
}

impl Tts for DummyTts {
    fn synth_text(&mut self, _input: &str) -> Result<Audio, TtsError> {
        Ok(Audio::new_empty(16000))
    }

    fn get_info(&self) -> TtsInfo {
        TtsInfo{
            name: "Dummy Synthesizer".to_string(),
            is_online: false
        }
    }
}

impl TtsStatic for DummyTts {
    fn check_compatible(_descr: &VoiceDescr) -> Result<(), TtsConstructionError> {
        // Just a dummy, won't output anything anyway
        Ok(())
    }
}

pub struct TtsFactory;

impl TtsFactory {
    pub fn load_with_prefs(lang: &LanguageIdentifier, prefer_cloud_tts: bool, gateway_key: Option<(String, String)>, prefs: &VoiceDescr) -> Result<Box<dyn Tts>, TtsConstructionError> {
        let local_tts = Box::new(PicoTts::new(lang, prefs)?);

        match prefer_cloud_tts {
            true => {
                if let Some((api_gateway, api_key)) = gateway_key {
                    Ok(Box::new(IbmTts::new(lang, local_tts, api_gateway.to_string(), api_key.to_string(), prefs)?))
                }
                else {
                    Ok(local_tts)
                }
            },
            false => {
                Ok(local_tts)
            }
        }
    }

    pub fn dummy() -> Box<dyn Tts> {
        Box::new(DummyTts::new())
    }
}