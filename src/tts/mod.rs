use core::fmt::Display;

mod error;
mod http_tts;
mod ibm;
mod larynx;
mod pico;

pub use self::error::*;
pub use self::ibm::*;
use self::larynx::LarynxData;
pub use self::pico::*;

#[cfg(feature = "extra_langs_tts")]
mod espeak;
#[cfg(feature = "extra_langs_tts")]
pub use self::espeak::*;
#[cfg(feature = "google_tts")]
mod google;
#[cfg(feature = "google_tts")]
pub use self::google::*;

use self::http_tts::HttpTts;

use async_trait::async_trait;
use fluent_langneg::{negotiate_languages, NegotiationStrategy};
use lily_common::audio::Audio;
use lily_common::other::false_val;
use serde::Deserialize;
use unic_langid::LanguageIdentifier;

// Traits //////////////////////////////////////////////////////////////////////
#[async_trait(?Send)]
pub trait Tts {
    async fn synth_text(&mut self, input: &str) -> Result<Audio, TtsError>;
    fn get_info(&self) -> TtsInfo;
}


pub trait TtsStatic {
    type Data;
    fn is_descr_compatible(d: &Self::Data, descr: &VoiceDescr) -> Result<(), TtsConstructionError>;
    fn is_lang_comptaible(d: &Self::Data, lang: &LanguageIdentifier) -> Result<(), TtsConstructionError>;
}

// Info ////////////////////////////////////////////////////////////////////////
#[derive(Debug, Clone)]
pub struct TtsInfo {
    pub name: String,
    pub is_online: bool,
}

impl Display for TtsInfo {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::result::Result<(), std::fmt::Error> {
        let online_str = {
            if self.is_online {
                "online"
            } else {
                "local"
            }
        };

        write!(formatter, "{}({})", self.name, online_str)
    }
}
// OnlineInterface /////////////////////////////////////////////////////////////
struct TtsOnlineInterface<O: Tts> {
    online: O,
    local: Box<dyn Tts>,
}

impl<O: Tts> TtsOnlineInterface<O> {
    pub fn new(online: O, local: Box<dyn Tts>) -> Self {
        Self { online, local }
    }
}

#[async_trait(?Send)]
impl<O: Tts> Tts for TtsOnlineInterface<O> {
    async fn synth_text(&mut self, input: &str) -> Result<Audio, TtsError> {
        match self.online.synth_text(input).await {
            Ok(audio) => Ok(audio),
            // If it didn't work try with local
            Err(_) => self.local.synth_text(input).await,
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
    Female,
}
#[derive(Debug, Clone)]
pub struct VoiceDescr {
    pub gender: Gender,
}

fn negotiate_langs_res(
    input: &LanguageIdentifier,
    available: &[LanguageIdentifier],
    default: Option<&LanguageIdentifier>,
) -> Result<LanguageIdentifier, TtsConstructionError> {
    let langs = negotiate_languages(&[input], available, default, NegotiationStrategy::Filtering);
    if !langs.is_empty() {
        Ok(langs[0].clone())
    } else {
        Err(TtsConstructionError::IncompatibleLanguage)
    }
}

// Conf ////////////////////////////////////////////////////////////////////////
#[derive(Clone, Debug, Deserialize)]
pub struct TtsData {
    #[serde(default = "false_val")]
    pub prefer_male: bool,
    #[serde(default = "false_val")]
    pub prefer_online: bool,

    #[serde(default)]
    pub ibm: Option<IbmTtsData>,

    #[serde(default)]
    pub larynx: Option<LarynxData>,
}

impl Default for TtsData {
    fn default() -> Self {
        Self {
            prefer_male: false,
            prefer_online: false,
            ibm: None,
            larynx: None
        }
    }
}

// Factory /////////////////////////////////////////////////////////////////////
pub struct TtsFactory;

impl TtsFactory {
    #[cfg(not(feature = "extra_langs_tts"))]
    fn make_local_tts(
        lang: &LanguageIdentifier,
        prefs: &VoiceDescr,
    ) -> Result<Box<dyn Tts>, TtsConstructionError> {
        Ok(Box::new(PicoTts::new(lang, prefs)?))
    }

    #[cfg(feature = "extra_langs_tts")]
    fn make_local_tts(
        lang: &LanguageIdentifier,
        prefs: &VoiceDescr,
    ) -> Result<Box<dyn Tts>, TtsConstructionError> {
        if PicoTts::is_descr_compatible(&(), prefs).is_ok() & PicoTts::is_lang_comptaible(&(), lang).is_ok() {
            Ok(Box::new(PicoTts::new(lang, prefs)?))
        } else {
            Ok(Box::new(EspeakTts::new(lang, prefs)))
        }
    }

    #[cfg(not(feature = "google_tts"))]
    fn make_cloud_tts(
        lang: &LanguageIdentifier,
        gateway_key: Option<IbmTtsData>,
        prefs: &VoiceDescr,
        local: Box<dyn Tts>,
    ) -> Result<Box<dyn Tts>, TtsConstructionError> {
        if let Some(ibm_data) = gateway_key {
            Ok(Box::new(TtsOnlineInterface::new(
                HttpTts::new(lang, prefs, ibm_data)?,
                local,
            )))
        } else {
            Ok(local)
        }
    }

    #[cfg(feature = "google_tts")]
    fn make_cloud_tts(
        lang: &LanguageIdentifier,
        gateway_key: Option<IbmTtsData>,
        prefs: &VoiceDescr,
        local: Box<dyn Tts>,
    ) -> Result<Box<dyn Tts>, TtsConstructionError> {
        if let Some(ibm_data) = gateway_key {
            Ok(Box::new(TtsOnlineInterface::new(
                HttpTts::new(lang, prefs, ibm_data)?,
                local,
            )))
        } else {
            Ok(Box::new(TtsOnlineInterface::new(
                HttpTts::new(lang, prefs, GttsData())?,
                local,
            )))
        }
    }

    pub fn load_with_prefs(
        lang: &LanguageIdentifier,
        prefer_cloud_tts: bool,
        gateway_key: Option<IbmTtsData>,
        prefs: &VoiceDescr,
    ) -> Result<Box<dyn Tts>, TtsConstructionError> {
        let local_tts = Self::make_local_tts(lang, prefs)?;

        match prefer_cloud_tts {
            true => Self::make_cloud_tts(lang, gateway_key, prefs, local_tts),
            false => Ok(local_tts),
        }
    }
}
