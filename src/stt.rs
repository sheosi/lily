use crate::lang::Lang;

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
    pub fn new(lang: Lang) -> Pocketsphinx {
    	let iso_str = lang.iso_str().to_owned();
        let config = pocketsphinx::CmdLn::init(
            true,
            &[
                //"pocketsphinx",
                "-hmm",
                &(iso_str.clone() + "/acoustic-model"),
                "-lm",
                &(iso_str.clone() + "/language-model.lm.bin"),
                "-dict",
                &(iso_str.clone() + "/pronounciation-dictionary.dict"),
            ],
        )
        .unwrap();
        let decoder = pocketsphinx::PsDecoder::init(config);

        Pocketsphinx {
            decoder,
            is_speech_started: false
        }
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
    pub fn new(lang: Lang, fallback: Box<dyn Stt>) -> Self{
        IbmStt{engine: crate::gtts::IbmSttEngine::new(), model: Self::model_from_lang(lang).to_string(), fallback, copy_audio: crate::audio::Audio{buffer: Vec::new(), samples_per_second: 16000}}
    }

    fn model_from_lang(lang: Lang) -> &'static str {
        match lang {
            Lang::EsEs => {"es-ES_BroadbandModel"},
            Lang::EnUs => {"en-US_BroadbandModel"}
        }
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
	pub fn load(lang: Lang, prefer_cloud: bool) -> Box<dyn Stt>{

		let local_stt = Box::new(Pocketsphinx::new(lang));
        if prefer_cloud {
            Box::new(IbmStt::new(lang, local_stt))
        }
        else {
            local_stt
        }
	}
}