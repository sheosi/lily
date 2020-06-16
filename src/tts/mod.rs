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
use fluent_langneg::{negotiate_languages, NegotiationStrategy};
// Traits //////////////////////////////////////////////////////////////////////
pub trait Tts {
    fn synth_text(&mut self, input: &str) -> Result<Audio, TtsError>;
    fn get_info(&self) -> TtsInfo;
}

pub trait TtsStatic {
    fn is_descr_compatible(descr: &VoiceDescr) -> Result<(), TtsConstructionError>;
    fn is_lang_comptaible(lang: &LanguageIdentifier) -> Result<(), TtsConstructionError>;
}

// Info ////////////////////////////////////////////////////////////////////////
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
// OnlineInterface /////////////////////////////////////////////////////////////
struct TtsOnlineInterface<O: Tts> {
    online: O,
    local: Box<dyn Tts>
}

impl<O: Tts> TtsOnlineInterface<O> {
    pub fn new(online: O, local: Box<dyn Tts>) -> Self {
        Self {online, local}
    }
}

impl<O: Tts> Tts for TtsOnlineInterface <O> {
    fn synth_text(&mut self, input: &str) -> Result<Audio, TtsError> {
        match self.online.synth_text(input) {
            Ok(audio) => Ok(audio),
            // If it didn't work try with local
            Err(_) => self.local.synth_text(input)
        }
    }

    fn get_info(&self) -> TtsInfo {
        self.online.get_info()
    }
}

// Other ///////////////////////////////////////////////////////////////////////
#[derive(Debug, Clone, PartialEq)]
pub enum Gender {
    Male,
    Female
}
#[derive(Debug, Clone)]
pub struct VoiceDescr {
    pub gender: Gender
}

fn negotiate_langs_res(
    input: &LanguageIdentifier,
    available: &Vec<LanguageIdentifier>,
    default: Option<&LanguageIdentifier>
) -> Result<LanguageIdentifier, TtsConstructionError> {
    let langs = negotiate_languages(&[input], available, default, NegotiationStrategy::Filtering);
    if !langs.is_empty() {
        Ok(langs[0].clone())
    }
    else {
        Err(TtsConstructionError::IncompatibleLanguage)
    }

}

// Dummy ///////////////////////////////////////////////////////////////////////
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
    fn is_descr_compatible(_descr: &VoiceDescr) -> Result<(), TtsConstructionError> {
        // Just a dummy, won't output anything anyway
        Ok(())
    }

    fn is_lang_comptaible(_lang: &LanguageIdentifier) -> Result<(), TtsConstructionError> {
        // Just a dummy, won't output anything anyway
        Ok(())
    }
}

// Factory /////////////////////////////////////////////////////////////////////
pub struct TtsFactory;

impl TtsFactory {
    #[cfg(not(feature = "extra_langs_tts"))]
    fn make_local_tts (lang: &LanguageIdentifier, prefs: &VoiceDescr) -> Result<Box<dyn Tts>, TtsConstructionError> {
        Ok(Box::new(PicoTts::new(lang, prefs)?))
    }

    #[cfg(feature = "extra_langs_tts")]
    fn make_local_tts (lang: &LanguageIdentifier, prefs: &VoiceDescr) -> Result<Box<dyn Tts>, TtsConstructionError> {
        if PicoTts::is_descr_compatible(prefs).is_ok() & PicoTts::is_lang_comptaible(lang).is_ok() {
            Ok(Box::new(PicoTts::new(lang, prefs)?))
        }
        else {
            Ok(Box::new(EspeakTts::new(lang, prefs)))
        }
    }

    #[cfg(not(feature = "google_tts"))]
    fn make_cloud_tts(lang: &LanguageIdentifier, gateway_key: Option<(String, String)>, prefs: &VoiceDescr, local: Box<dyn Tts>) -> Result<Box<dyn Tts>, TtsConstructionError> {
        if let Some((api_gateway, api_key)) = gateway_key {
            Ok(Box::new(TtsOnlineInterface::new(IbmTts::new(lang, api_gateway.to_string(), api_key.to_string(), prefs)?, local)))
        }
        else {
            Ok(local)
        }
    }

    #[cfg(feature = "google_tts")]
    fn make_cloud_tts(lang: &LanguageIdentifier, gateway_key: Option<(String, String)>, prefs: &VoiceDescr, local: Box<dyn Tts>) -> Result<Box<dyn Tts>, TtsConstructionError> {
        if let Some((api_gateway, api_key)) = gateway_key {
            Ok(Box::new(TtsOnlineInterface::new(IbmTts::new(lang, api_gateway.to_string(), api_key.to_string(), prefs)?, local)))
        }
        else {
            Ok(Box::new(TtsOnlineInterface::new(GTts::new(lang), local)))
        }
    }

    pub fn load_with_prefs(lang: &LanguageIdentifier, prefer_cloud_tts: bool, gateway_key: Option<(String, String)>, prefs: &VoiceDescr) -> Result<Box<dyn Tts>, TtsConstructionError> {
        let local_tts = Self::make_local_tts(lang, prefs)?;

        match prefer_cloud_tts {
            true => {
                Self::make_cloud_tts(lang, gateway_key, prefs, local_tts)
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