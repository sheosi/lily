mod bundles;
mod error;
mod ibm;
mod pocketsphinx;

#[cfg(feature = "deepspeech_stt")]
mod deepspeech;

pub use self::bundles::*;
pub use self::error::*;
pub use self::ibm::*;
pub use self::pocketsphinx::*;

#[cfg(feature = "deepspeech_stt")]
pub use self::deepspeech::*;

use core::fmt::Display;
use crate::audio::AudioRaw;
use fluent_langneg::{negotiate_languages, NegotiationStrategy};
use unic_langid::LanguageIdentifier;
use log::info;

use crate::vad::SnowboyVad;
use crate::vars::SNOWBOY_DATA_PATH;


#[derive(Debug, Clone)]
pub struct SttInfo {
    pub name: String,
    pub is_online: bool
}

impl Display for SttInfo {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::result::Result<(), std::fmt::Error> {
        let online_str = {
            if self.is_online {"online"}
            else {"local"}

        };
        
        write!(formatter, "{}({})", self.name, online_str)
    }
}

#[derive(PartialEq, Debug)]
pub enum DecodeState {
    NotStarted, 
    StartListening,
    NotFinished,
    Finished(Option<DecodeRes>),
}

#[derive(PartialEq, Debug)]
pub struct DecodeRes {
    pub hypothesis: String
}

// An Stt which accepts an Stream
pub trait SttStream {
    fn begin_decoding(&mut self) -> Result<(),SttError>;
    fn decode(&mut self, audio: &[i16]) -> Result<DecodeState, SttError>;
    fn get_info(&self) -> SttInfo;
}

// An Stt which accepts only audio batches
pub trait SttBatched {
    fn decode(&mut self, audio: &[i16]) -> Result<Option<DecodeRes>, SttError>;
    fn get_info(&self) -> SttInfo;
}

pub trait SttVadless {
    fn begin_decoding(&mut self) -> Result<(), SttError>;
    fn process(&mut self, audio: &[i16]) -> Result<(), SttError>;
    fn end_decoding(&mut self) -> Result<Option<DecodeRes>, SttError>;
    fn get_info(&self) -> SttInfo;
}


pub struct SttFactory;

pub trait SpecifiesLangs {
    fn available_langs() -> Vec<LanguageIdentifier>;
}

pub trait IsLangCompatible {
    fn is_lang_compatible(lang: &LanguageIdentifier) -> Result<(), SttConstructionError>;
}

impl<T> IsLangCompatible for T where T: SpecifiesLangs {
    fn is_lang_compatible(lang: &LanguageIdentifier) -> Result<(), SttConstructionError> {
        negotiate_langs_res(lang, &Self::available_langs()).map(|_|())
    }
}

fn negotiate_langs_res(
    input: &LanguageIdentifier,
    available: &Vec<LanguageIdentifier>
) -> Result<LanguageIdentifier, SttConstructionError> {
    let langs = negotiate_languages(&[input], available, None, NegotiationStrategy::Filtering);
    if !langs.is_empty() {
        Ok(langs[0].clone())
    }
    else {
        Err(SttConstructionError::LangIncompatible)
    }

}
const DYNAMIC_ENERGY_ADJUSTMENT_DAMPING: f64 = 0.15;
const DYNAMIC_ENERGY_RATIO: f64 = 0.00013;
const MIN_ENERGY: f64 = 3.0;
fn calc_threshold(audio: &AudioRaw) -> f64 {
    // This is taken from python's speech_recognition package
    let energy = audio.rms();
    let damping = DYNAMIC_ENERGY_ADJUSTMENT_DAMPING.powf(audio.len_s() as f64);
    let target_energy = energy * DYNAMIC_ENERGY_RATIO;
    let res = MIN_ENERGY * damping + target_energy;
    println!("{}, damping {}, target {}", res, damping, target_energy);

    res
}

impl SttFactory {
    #[cfg(feature = "deepspeech_stt")]
    fn make_local(lang: &LanguageIdentifier, audio_sample: &AudioRaw) -> Result<Box<dyn SttStream>, SttConstructionError> {
        if DeepSpeechStt::is_lang_compatible(lang).is_ok() {
            Ok(Box::new(SttVadlessInterface::new(DeepSpeechStt::new(lang)?, SnowboyVad::new(&SNOWBOY_DATA_PATH.resolve().join("common.res"))?)))
        }
        else {
            Ok(Box::new(Pocketsphinx::new(lang, audio_sample)?))
        }
    }

    #[cfg(not(feature = "deepspeech_stt"))]
    fn make_local(lang: &LanguageIdentifier, audio_sample: &AudioRaw) -> Result<Box<dyn SttStream>, SttConstructionError> {
        Ok(Box::new(Pocketsphinx::new(lang, audio_sample)?))
    }


	pub fn load(lang: &LanguageIdentifier, audio_sample: &AudioRaw, prefer_cloud: bool, ibm_data: Option<IbmSttData>) -> Result<Box<dyn SttStream>, SttConstructionError> {

		let local_stt = Self::make_local(lang, audio_sample)?;
        if prefer_cloud {
            info!("Prefer online Stt");
            if let Some(ibm_data_obj) = ibm_data {
                info!("Construct online Stt");
                let vad = SnowboyVad::new(&SNOWBOY_DATA_PATH.resolve().join("common.res"))?;
                let online = SttVadlessInterface::new(IbmStt::new(lang,ibm_data_obj)?,vad);
                //let online = SttBatcher::new(IbmStt::new(lang,ibm_data_obj)?,vad);
                Ok(Box::new(SttFallback::new(online, local_stt)))
            }
            else {
                Ok(local_stt)
            }
        }
        else {
            Ok(local_stt)
        }
    }
}