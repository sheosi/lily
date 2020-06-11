use crate::audio::Audio;
use crate::tts::{OnlineTtsError, Tts, TtsStatic, TtsError, TtsInfo, VoiceDescr, TtsConstructionError, negotiate_langs_res};
use crate::vars::NO_COMPATIBLE_LANG_MSG;
use unic_langid::{LanguageIdentifier, langid, langids};

pub struct GttsEngine {
	client: reqwest::blocking::Client
}


impl GttsEngine {
	pub fn new() -> Self {
		GttsEngine{client: reqwest::blocking::Client::new()}
	}

	// This one will return an MP3
	pub fn synth(&mut self, text: &str, lang: &str) -> Result<Vec<u8>, OnlineTtsError> {
		const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; WOW64) \
	            AppleWebKit/537.36 (KHTML, like Gecko) \
	            Chrome/47.0.2526.106 Safari/537.36";

	    let url = google_translate_tts::url(text, lang);
	    log::info!("{}", url);

	    let mut buf: Vec<u8> = vec![];
	    self.client.get(&url).header("Referer", "http://translate.google.com/").header("User-Agent", USER_AGENT).send()?
	    .copy_to(&mut buf)?;

		Ok(buf)
	}
}

struct GTts {
    engine: GttsEngine,
    fallback_tts : Box<dyn Tts>,
    curr_lang: String
}

impl GTts {

    pub fn new(lang: &LanguageIdentifier, fallback_tts: Box<dyn Tts>) -> Self {
        GTts{engine: GttsEngine::new(), fallback_tts, curr_lang: Self::make_tts_lang(&Self::lang_neg(lang)).to_string()}
    }

    fn make_tts_lang<'a>(lang: &'a LanguageIdentifier) -> &'a str {
        lang.language.as_str()
    }

    fn lang_neg(lang: &LanguageIdentifier) -> LanguageIdentifier {
        let default = langid!("en");
        negotiate_langs_res(lang, &Self::available_langs(), Some(&default)).expect(NO_COMPATIBLE_LANG_MSG)
    }

    fn available_langs() -> Vec<LanguageIdentifier> {
        langids!("es", "en")
    }
}

impl Tts for GTts {
    fn synth_text(&mut self, input: &str) -> Result<Audio, TtsError> {
        match self.engine.synth(input, &self.curr_lang) {
            Ok(buffer) => {Ok(Audio::new_encoded(buffer, 16000))},
            Err(_) => {
                // If it didn't work try with local
                self.fallback_tts.synth_text(input)
            }
        }
    }

    fn get_info(&self) -> TtsInfo {
        TtsInfo {
            name: "Google Translate".to_string(),
            is_online: true
        }
    }
}

impl TtsStatic for GTts {
    fn is_descr_compatible(_descr: &VoiceDescr) -> Result<(), TtsConstructionError> {
        Ok(())
    }

    fn is_lang_comptaible(lang: &LanguageIdentifier) -> Result<(), TtsConstructionError> {
        negotiate_langs_res(lang, &Self::available_langs(), None).map(|_|())
    }
}
