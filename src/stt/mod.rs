mod bundles;
mod deepspeech;
mod error;
mod ibm;
mod pocketsphinx;

pub use self::bundles::*;
pub use self::deepspeech::*;
pub use self::error::*;
pub use self::ibm::*;
pub use self::pocketsphinx::*;

use core::fmt::Display;
use unic_langid::LanguageIdentifier;
use log::info;

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
    Finished(Option<(String, Option<String>, i32)>),
}

// An Stt which accepts an Stream
pub trait SttStream {
    fn begin_decoding(&mut self) -> Result<(),SttError>;
    fn decode(&mut self, audio: &[i16]) -> Result<DecodeState, SttError>;
    fn get_info(&self) -> SttInfo;
}

// An Stt which accepts only audio batches
pub trait SttBatched {
    fn decode(&mut self, audio: &[i16]) -> Result<Option<(String, Option<String>, i32)>, SttError>;
    fn get_info(&self) -> SttInfo;
}

pub trait SttVadless {
    fn process(&mut self, audio: &[i16]) -> Result<(), SttError>;
    fn end_decoding(&mut self) -> Result<Option<(String, Option<String>, i32)>, SttError>;
    fn get_info(&self) -> SttInfo;
}


pub struct SttFactory;

impl SttFactory {
	pub fn load(lang: &LanguageIdentifier, prefer_cloud: bool, gateway_key: Option<(String, String)>) -> Result<Box<dyn SttStream>, SttConstructionError> {

		let local_stt = Pocketsphinx::new(lang)?;
        if prefer_cloud {
            info!("Prefer online Stt");
            if let Some((api_gateway, api_key)) = gateway_key {
                info!("Construct online Stt");
                Ok(Box::new(SttOnlineInterface::new(IbmStt::new(lang, api_gateway.to_string(), api_key.to_string())?, local_stt)))
            }
            else {
                Ok(Box::new(local_stt))
            }
        }
        else {
            Ok(Box::new(local_stt))
        }
	}
}

