use crate::exts::ToStrResult;
use crate::tts::{
    negotiate_langs_res, Gender, Tts, TtsConstructionError, TtsError, TtsInfo, TtsStatic,
    VoiceDescr,
};
use crate::vars::{NO_COMPATIBLE_LANG_MSG, PICO_DATA_PATH};

use async_trait::async_trait;
use lily_common::audio::Audio;
use ttspico as pico;
use unic_langid::{langid, langids, LanguageIdentifier};

pub struct PicoTts {
    engine: pico::Engine,
}

impl PicoTts {
    // Just accept a negotiated LanguageIdentifier
    fn sg_name(lang: &LanguageIdentifier) -> Result<&'static str, TtsConstructionError> {
        let lang_str = format!(
            "{}-{}",
            lang.language,
            lang.region.ok_or(TtsConstructionError::NoRegion)?
        );
        match lang_str.as_str() {
            "es-ES" => Ok("es-ES_zl0_sg.bin"),
            "en-US" => Ok("en-US_lh0_sg.bin"),
            _ => Err(TtsConstructionError::IncompatibleLanguage),
        }
    }

    fn ta_name(lang: &LanguageIdentifier) -> Result<String, TtsConstructionError> {
        Ok(format!(
            "{}-{}_ta.bin",
            lang.language,
            lang.region.ok_or(TtsConstructionError::NoRegion)?.as_str()
        ))
    }

    // There's only one voice of Pico per language so preferences are not of much use here
    pub fn new(
        lang: &LanguageIdentifier,
        prefs: &VoiceDescr,
    ) -> Result<Self, TtsConstructionError> {
        Self::is_descr_compatible(prefs)?; // Check voice description compatibility

        // 1. Create a Pico system
        let lang = Self::lang_neg(lang);
        let sys = pico::System::new(4 * 1024 * 1024).expect("Could not init system");
        let lang_path = PICO_DATA_PATH.resolve();

        // 2. Load Text Analysis (TA) and Speech Generation (SG) resources for the voice you want to use
        let ta_res = pico::System::load_resource(
            sys.clone(),
            lang_path.join(Self::ta_name(&lang)?).to_str_res()?,
        )
        .expect("Failed to load TA");
        let sg_res = pico::System::load_resource(
            sys.clone(),
            lang_path.join(Self::sg_name(&lang)?).to_str_res()?,
        )
        .expect("Failed to load SG");

        // 3. Create a Pico voice definition and attach the loaded resources to it
        let voice =
            pico::System::create_voice(sys.clone(), "TestVoice").expect("Failed to create voice");
        voice
            .borrow_mut()
            .add_resource(ta_res)
            .expect("Failed to add TA to voice");
        voice
            .borrow_mut()
            .add_resource(sg_res)
            .expect("Failed to add SG to voice");

        // 4. Create an engine from the voice definition
        // UNSAFE: Creating an engine without attaching the resources will result in a crash!
        let engine =
            unsafe { pico::Voice::create_engine(voice.clone()).expect("Failed to create engine") };
        //let voice_def = espeak_sys::espeak_VOICE{name: std::ptr::null(), languages: std::ptr::null(), identifier: std::ptr::null(), gender: 0, age: 0, variant: 0, xx1:0, score: 0, spare: std::ptr::null_mut()};
        Ok(PicoTts { engine })
    }

    fn lang_neg(lang: &LanguageIdentifier) -> LanguageIdentifier {
        let default = langid!("en-US");

        negotiate_langs_res(lang, &Self::available_langs(), Some(&default))
            .expect(NO_COMPATIBLE_LANG_MSG)
    }

    fn available_langs() -> Vec<LanguageIdentifier> {
        langids!("es-ES", "en-US")
    }
}

#[async_trait(?Send)]
impl Tts for PicoTts {
    async fn synth_text(&mut self, input: &str) -> Result<Audio, TtsError> {
        // 5. Put (UTF-8) text to be spoken into the engine
        // See `Engine::put_text()` for more details.
        let input = std::ffi::CString::new(input).expect("CString::new failed");
        let mut text_bytes = input.as_bytes_with_nul();
        while !text_bytes.is_empty() {
            let n_put = self
                .engine
                .put_text(text_bytes)
                .expect("pico_putTextUtf8 failed");
            text_bytes = &text_bytes[n_put..];
        }

        // 6. Do the actual text-to-speech, getting audio data (16-bit signed PCM @ 16kHz) from the input text
        // Speech audio is computed in small chunks, one "step" at a time; see `Engine::get_data()` for more details.
        let mut pcm_data = vec![0i16; 0];
        let mut pcm_buf = [0i16; 1024];
        loop {
            let (n_written, status) = self
                .engine
                .get_data(&mut pcm_buf[..])
                .expect("pico_getData error");
            pcm_data.extend(&pcm_buf[..n_written]);
            if status == ttspico::EngineStatus::Idle {
                break;
            }
        }

        Ok(Audio::new_raw(pcm_data, 16000))
    }

    fn get_info(&self) -> TtsInfo {
        TtsInfo {
            name: "Pico Tts".to_string(),
            is_online: false,
        }
    }
}

impl TtsStatic for PicoTts {
    fn is_descr_compatible(descr: &VoiceDescr) -> Result<(), TtsConstructionError> {
        //Only has female voices (by default)
        if descr.gender != Gender::Female {
            Err(TtsConstructionError::WrongGender)
        } else {
            Ok(())
        }
    }

    fn is_lang_comptaible(lang: &LanguageIdentifier) -> Result<(), TtsConstructionError> {
        let default = langid!("en-US");

        negotiate_langs_res(lang, &Self::available_langs(), Some(&default)).map(|_| ())
    }
}
