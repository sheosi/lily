use core::fmt::Display;

use crate::vars::{STT_DATA_PATH, resolve_path};
use crate::vad::{Vad, VadError};
use crate::audio::AudioRaw;
use crate::path_ext::{NotUnicodeError, ToStrResult};

use unic_langid::{LanguageIdentifier, langid, langids};
use fluent_langneg::{negotiate_languages, NegotiationStrategy};
use log::info;
use thiserror::Error;

const SOUND_SPS: u32 = 16000;

#[derive(Error, Debug)]
pub enum SttError {
    #[error("PocketSphinx error, see log for details")]
    PocketsphinxError(#[from] pocketsphinx::Error),

    #[error("Error while sending to online service")]
    OnlineError(#[from] crate::gtts::GttsError),

    #[error("Error from the vad")]
    Vad(#[from] crate::vad::VadError)
}

#[derive(Error, Debug)]
pub enum SttConstructionError {
    #[error("The input language identifier has no region")]
    NoRegion,

    #[error("PocketSphinx error, see log for details")]
    PocketsphinxError(#[from] pocketsphinx::Error),

    #[error("Input path was not unicode")]
    NotUnicodeError(#[from] NotUnicodeError),

    #[cfg(feature = "devel_deepspeech")]
    #[error("Can't load files")]
    CantLoadFiles,

    #[error("Input is not a valid utf-8")]
    FromUtf8Error(#[from] std::string::FromUtf8Error)
}

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

pub struct Pocketsphinx {
    decoder: pocketsphinx::PsDecoder,
    is_speech_started: bool,
}


impl Pocketsphinx {
    pub fn new(lang: &LanguageIdentifier) -> Result<Self, SttConstructionError> {
        let lang = Self::lang_neg(lang);
        let iso_str = format!("{}-{}", lang.language(), lang.region().ok_or(SttConstructionError::NoRegion)?.to_lowercase());
        let stt_path = resolve_path(STT_DATA_PATH);

        let config = pocketsphinx::CmdLn::init(
            true,
            &[  
                //"pocketsphinx",
                "-hmm",
                stt_path.join(&iso_str).join(&iso_str).to_str_res()?,
                "-lm",
                stt_path.join(&iso_str).join(iso_str.to_string() + ".lm.bin").to_str_res()?,
                "-dict",
                stt_path.join(&iso_str).join("cmudict-".to_owned() + &iso_str + ".dict").to_str_res()?,
                "-logfn", "nul", // This is to silence pocketpshinx, however without it it spits all params
                                 // so is pretty useful
                "-vad_threshold", "3.5"
            ],
        )?;
        let decoder = pocketsphinx::PsDecoder::init(config);

        Ok(Pocketsphinx {
            decoder,
            is_speech_started: false
        })
    }

    fn lang_neg(lang: &LanguageIdentifier) -> LanguageIdentifier {
        let available_langs = langids!("es-ES", "en-US");
        let default = langid!("en-US");
        negotiate_languages(&[lang], &available_langs, Some(&default), NegotiationStrategy::Filtering)[0].clone()
    }
}

impl SttStream for Pocketsphinx {
    fn begin_decoding(&mut self) -> Result<(), SttError> {
        self.decoder.start_utt(None)?;
        Ok(())
    }   
    fn decode(&mut self, audio: &[i16]) -> Result<DecodeState, SttError> {
        self.decoder.process_raw(audio, false, false)?;
        if self.decoder.get_in_speech() {
            if !self.is_speech_started {
                //begining of utterance
                self.is_speech_started = true;
                Ok(DecodeState::StartListening)
            } else {
                Ok(DecodeState::NotFinished)
            }
        } else {
            if self.is_speech_started {
                self.is_speech_started = false;
                self.decoder.end_utt()?;
                Ok(DecodeState::Finished(self.decoder.get_hyp()))
            } else {
                // TODO: Check this
                Ok(DecodeState::NotStarted)
            }
        }
    }
    fn get_info(&self) -> SttInfo {
        SttInfo {name: "Pocketsphinx".to_string(), is_online: false}
    }
}

impl Vad for Pocketsphinx {
    fn reset(&mut self) -> Result<(), VadError> {
        self.begin_decoding().map_err(|_|VadError::Unknown)?;
        Ok(())
    }

    fn is_someone_talking(&mut self, audio: &[i16]) -> Result<bool, VadError> {
        self.decode(audio).map_err(|_|VadError::Unknown)?;
        Ok(self.is_speech_started)
    }
}

pub struct SttBatcher<V: Vad, S: SttBatched> {
    vad: V,
    copy_audio: crate::audio::AudioRaw,
    batch_stt: S,
    someone_was_talking: bool
}

impl<V: Vad, S: SttBatched> SttBatcher<V, S> {
    fn new(vad: V, batch_stt: S) -> Self {
        Self {vad, copy_audio: crate::audio::AudioRaw::new_empty(SOUND_SPS), batch_stt, someone_was_talking: false}
    }
}



impl<V: Vad, S: SttBatched> SttStream for SttBatcher<V, S> {
    fn begin_decoding(&mut self) -> Result<(),SttError> {
        self.copy_audio.clear();
        self.vad.reset()?;
        self.someone_was_talking = false;

        Ok(())
    }

    fn decode(&mut self, audio: &[i16]) -> Result<DecodeState, SttError> {
        self.copy_audio.append_audio(audio, SOUND_SPS);
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
    copy_audio: crate::audio::AudioRaw,
}

impl<S: SttBatched, F: SttStream> SttOnlineInterface<S, F> {
    fn new(online_stt: S,fallback: F) -> Self {
        Self{online_stt, fallback, copy_audio: AudioRaw::new_empty(SOUND_SPS)}
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
            self.copy_audio.append_audio(audio, SOUND_SPS);
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


pub struct IbmStt {
    engine: crate::gtts::IbmSttEngine,
    model: String
}

impl IbmStt {

    pub fn new(lang: &LanguageIdentifier, api_gateway: String, api_key: String) -> Result<Self, SttConstructionError> {
        Ok(IbmStt{engine: crate::gtts::IbmSttEngine::new(api_gateway, api_key), model: Self::model_from_lang(lang)?.to_string()})
    }

    fn model_from_lang(lang: &LanguageIdentifier) -> Result<String, SttConstructionError> {
        let lang = Self::lang_neg(lang);
        Ok(format!("{}-{}_BroadbandModel", lang.language(), lang.region().ok_or(SttConstructionError::NoRegion)?))
    }

    fn lang_neg(lang: &LanguageIdentifier) -> LanguageIdentifier {
        let available_langs = langids!("es-ES", "en-US");
        let default = langid!("en-US");
        negotiate_languages(&[lang],&available_langs, Some(&default), NegotiationStrategy::Filtering)[0].clone()
    }
}

impl SttBatched for IbmStt {
    
    fn decode(&mut self, audio: &[i16]) -> Result<Option<(String, Option<String>, i32)>, SttError> {
        Ok(self.engine.decode(&AudioRaw::new_raw(audio.to_vec(), SOUND_SPS), &self.model)?)
    }

    fn get_info(&self) -> SttInfo {
        SttInfo {name: "Ibm's Speech To Text".to_string(), is_online: true}
    }
}

impl SttVadless for IbmStt {
    fn process(&mut self, audio: &[i16]) -> Result<(), SttError> {
        self.engine.live_process(&AudioRaw::new_raw(audio.to_vec(), 16000), &self.model)?;
        Ok(())
    }
    fn end_decoding(&mut self) -> Result<Option<(String, Option<String>, i32)>, SttError> {
        println!("End decode ");
        let res = self.engine.live_process_end(&self.model)?;
        Ok(res)
    }
    fn get_info(&self) -> SttInfo {
        SttInfo {name: "Ibm's Speech To Text".to_string(), is_online: true}
    }
}

pub struct SttVadlessInterface<S: SttVadless, F: SttStream> {
    online_stt: S,
    fallback: F,
}


impl<S: SttVadless, F: SttStream> SttVadlessInterface<S, F> {
    fn new(online_stt: S,fallback: F) -> Self {
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

// Deepspeech
#[cfg(feature = "devel_deepspeech")]
pub struct DeepSpeechStt {
    model: deepspeech::Model
}

#[cfg(feature = "devel_deepspeech")]
impl DeepspeechStt { 
    pub fn new() -> Result<Self, SttConstructionError> { 
        const BEAM_WIDTH:u16 = 500;
        const LM_WEIGHT:f32 = 16_000;

        let mut model = deepspeech::Model::load_from_files(&dir_path.join("output_graph.pb"), BEAM_WIDTH).map_err(|_| SttError::CantLoadFiles)?;
        model.enable_decoder_with_lm(&dir_path.join("lm.binary"),&dir_path.join("trie"), LM_WEIGHT, VALID_WORD_COUNT_WEIGHT);

        Self {model}
    }
}

#[cfg(feature = "devel_deepspeech")]
impl SttBatched for DeepspeechStt {
    fn decode(&mut self, audio: &[i16]) -> Result<DecodeState, SttError> {
        Ok(DecodeState::Finished(m.speech_to_text(&audio_buf)?))
    }

    fn get_info() -> SttInfo {
        SttInfo {name: "DeepSpeech", is_online: false}
    }
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

