use crate::audio::AudioRaw;
use crate::stt::{calc_threshold, DecodeState, SttConstructionError, SttError, SttStream, SttInfo};
use crate::vad::{Vad, VadError};
use crate::vars::*;
use crate::path_ext::ToStrResult;

use pocketsphinx::{PsDecoder, CmdLn};
use fluent_langneg::{negotiate_languages, NegotiationStrategy};
use unic_langid::{LanguageIdentifier, langid, langids};
pub struct Pocketsphinx {
    decoder: PsDecoder,
    is_speech_started: bool,
}



impl Pocketsphinx {
    pub fn new(lang: &LanguageIdentifier, audio_sample: &AudioRaw) -> Result<Self, SttConstructionError> {
        let lang = Self::lang_neg(lang);
        let iso_str = format!("{}-{}", lang.language, lang.region.ok_or(SttConstructionError::NoRegion)?.as_str().to_lowercase());
        let stt_path = STT_DATA_PATH.resolve();
        let ener_threshold = calc_threshold(audio_sample);

        let config = CmdLn::init( 
            true,
            &[  
                //"pocketsphinx",
                "-hmm",
                stt_path.join(&iso_str).join(&iso_str).to_str_res()?,
                "-lm",
                stt_path.join(&iso_str).join(iso_str.to_string() + ".lm.bin").to_str_res()?,
                "-dict",
                stt_path.join(&iso_str).join("cmudict-".to_owned() + &iso_str + ".dict").to_str_res()?,
                "-logfn", "nul",
                "-vad_threshold", &ener_threshold.to_string()

            ]
        )?;
        let decoder = PsDecoder::init(config);

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