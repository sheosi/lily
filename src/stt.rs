use std::path::Path;
use unic_langid::{LanguageIdentifier, langid, langids};
use crate::vars::STT_DATA_PATH;
use fluent_langneg::{negotiate_languages, NegotiationStrategy};

#[derive(Debug, Clone)]
pub enum SttErrCause {
    UNKNOWN
}

#[derive(Debug, Clone)]
pub struct SttError {
    cause: SttErrCause
}


pub enum DecodeState {
    NotStarted, 
    StartListening,
    NotFinished,
    Finished(Option<(String, Option<String>, i32)>),
}


pub trait Stt {
    fn begin_decoding(&mut self) -> Result<(),SttError>;
    fn decode(&mut self, audio: &[i16]) -> Result<DecodeState, SttError>;
}

pub struct Pocketsphinx {
    decoder: pocketsphinx::PsDecoder,
    is_speech_started: bool,
}

impl Pocketsphinx {
    pub fn new(lang: &LanguageIdentifier) -> Self {
        let lang = Self::lang_neg(lang);
        let iso_str = format!("{}-{}", lang.get_language(), lang.get_region().unwrap().to_lowercase());
        let stt_path = Path::new(STT_DATA_PATH);

        let config = pocketsphinx::CmdLn::init(
            true,
            &[
                //"pocketsphinx",
                "-hmm",
                stt_path.join(&iso_str).join(&iso_str).to_str().unwrap(),
                "-lm",
                stt_path.join(&iso_str).join(iso_str.to_string() + ".lm.bin").to_str().unwrap(),
                "-dict",
                stt_path.join(&iso_str).join("cmudict-".to_owned() + &iso_str + ".dict").to_str().unwrap(),
            ],
        )
        .unwrap();
        let decoder = pocketsphinx::PsDecoder::init(config);

        Pocketsphinx {
            decoder,
            is_speech_started: false
        }
    }

    fn lang_neg(lang: &LanguageIdentifier) -> LanguageIdentifier {
        let available_langs = langids!("es-ES", "en-US");
        let default = langid!("en-US");
        negotiate_languages(&[lang], &available_langs, Some(&default), NegotiationStrategy::Filtering)[0].clone()
    }
}

impl std::fmt::Display for SttError {
    fn fmt (&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self.cause {
            SttErrCause::UNKNOWN => {
                write!(f, "PocketSphinx error, see log for details")
            }
        }
    }
}

impl std::convert::From<pocketsphinx::Error> for SttError {
    fn from(_err: pocketsphinx::Error) -> Self {
        SttError{cause: SttErrCause::UNKNOWN}
    }
}

impl Stt for Pocketsphinx {
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
                Ok(DecodeState::NotStarted)
            }
        } else {
            if self.is_speech_started {
                self.is_speech_started = false;
                self.decoder.end_utt()?;
                Ok(DecodeState::Finished(self.decoder.get_hyp()))
            } else {
                // TODO: Check this
                Ok(DecodeState::NotFinished)
            }
        }
    }
}


pub struct IbmStt {
    engine: crate::gtts::IbmSttEngine,
    model: String,
    fallback: Box<dyn Stt>,
    copy_audio: crate::audio::Audio
}

impl IbmStt {


    pub fn new(lang: &LanguageIdentifier, fallback: Box<dyn Stt>) -> Self{
        IbmStt{engine: crate::gtts::IbmSttEngine::new(), model: Self::model_from_lang(lang).to_string(), fallback, copy_audio: crate::audio::Audio{buffer: Vec::new(), samples_per_second: 16000}}
    }

    fn model_from_lang(lang: &LanguageIdentifier) -> String {
        let lang = Self::lang_neg(lang);
        format!("{}-{}_BroadbandModel", lang.get_language(), lang.get_region().unwrap())
    }

    fn lang_neg(lang: &LanguageIdentifier) -> LanguageIdentifier {
        let available_langs = langids!("es-ES", "en-US");
        let default = langid!("en-US");
        negotiate_languages(&[lang],&available_langs, Some(&default), NegotiationStrategy::Filtering)[0].clone()
    }
}

impl Stt for IbmStt {
    fn begin_decoding(&mut self) -> Result<(),SttError> {
        self.copy_audio.clear();
        self.fallback.begin_decoding()?;
        Ok(())
    }
    
    fn decode(&mut self, audio: &[i16]) -> Result<DecodeState, SttError> {
        self.copy_audio.append_audio(audio, 16000);
        let res = self.fallback.decode(audio)?;
        match res {
            DecodeState::Finished(local_res) => {
                match self.engine.decode(&self.copy_audio, &self.model) {
                    Ok(ok_res) => Ok(DecodeState::Finished(Some(ok_res))),
                    Err(_) => Ok(DecodeState::Finished(local_res))
                }

            },
            _ => Ok(res)
        }
    }
}

pub struct SttFactory;

impl SttFactory {
	pub fn load(lang: &LanguageIdentifier, prefer_cloud: bool) -> Box<dyn Stt>{

		let local_stt = Box::new(Pocketsphinx::new(lang));
        if prefer_cloud {
            Box::new(IbmStt::new(lang, local_stt))
        }
        else {
            local_stt
        }
	}
}