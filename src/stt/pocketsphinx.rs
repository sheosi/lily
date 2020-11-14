use crate::stt::{calc_threshold, DecodeRes, Stt, SttConstructionError, SttError, SttInfo};
use crate::vars::*;
use crate::path_ext::ToStrResult;

use async_trait::async_trait;
use fluent_langneg::{negotiate_languages, NegotiationStrategy};
use lily_common::audio::AudioRaw;
use pocketsphinx::{PsDecoder, CmdLn};
use unic_langid::{LanguageIdentifier, langid, langids};
pub struct Pocketsphinx {
    decoder: PsDecoder
}

impl Pocketsphinx {
    pub fn new(lang: &LanguageIdentifier, audio_sample: &AudioRaw) -> Result<Self, SttConstructionError> {
        let lang = Self::lang_neg(lang);
        let iso_str = format!("{}-{}", lang.language, lang.region.ok_or(SttConstructionError::NoRegion)?.as_str().to_lowercase());
        let stt_path = STT_DATA_PATH.resolve();
        let ener_threshold = calc_threshold(audio_sample);

        let ps_log = PS_LOG_PATH.resolve();
        let ps_log_str = ps_log.to_str().expect("Pocketsphinx path is not UTF-8 compatible, this is not supported");
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
                "-logfn", ps_log_str,
                "-vad_threshold", &ener_threshold.to_string()

            ]
        )?;
        let decoder = PsDecoder::init(config);

        Ok(Pocketsphinx {
            decoder
        })
    }

    fn lang_neg(lang: &LanguageIdentifier) -> LanguageIdentifier {
        let available_langs = langids!("es-ES", "en-US");
        let default = langid!("en-US");
        negotiate_languages(&[lang], &available_langs, Some(&default), NegotiationStrategy::Filtering)[0].clone()
    }
}

impl Pocketsphinx {
    fn base_begin(&mut self) -> Result<(), SttError> {
        self.decoder.start_utt(None)?;
        Ok(())
    }
}

#[async_trait(?Send)]
impl Stt for Pocketsphinx {
    async fn begin_decoding(&mut self) -> Result<(), SttError> {
        self.base_begin()
    }
    async fn process(&mut self, audio: &[i16]) -> Result<(), SttError> {
        self.decoder.process_raw(audio, false, false)?;
        Ok(())
    }
    async fn end_decoding(&mut self) -> Result<Option<DecodeRes>, SttError> {
        Ok(self.decoder.get_hyp().map(|(hypothesis, _, _)| DecodeRes{hypothesis}))
    }
    fn get_info(&self) -> SttInfo {
        SttInfo {name: "Pocketsphinx".to_string(), is_online: false}
    }
}