use crate::tts::{Tts, VoiceDescr, TtsConstructionError, Gender, TtsError, TtsInfo, TtsStatic, OnlineTtsError, negotiate_langs_res};
use crate::audio::Audio;
use crate::vars::{NO_COMPATIBLE_LANG_MSG, DEFAULT_SAMPLES_PER_SECOND};

use unic_langid::{LanguageIdentifier, langid, langids};

pub struct IbmTts {
    engine: IbmTtsEngine,
    curr_voice: String
}

impl IbmTts {
    pub fn new(lang: &LanguageIdentifier, api_gateway: String, api_key: String, prefs: &VoiceDescr) -> Result<Self, TtsConstructionError> {
        Ok(IbmTts{engine: IbmTtsEngine::new(api_gateway, api_key), curr_voice: Self::make_tts_voice(&Self::lang_neg(lang), prefs)?.to_string()})
    }

    // Accept only negotiated LanguageIdentifiers
    fn make_tts_voice(lang: &LanguageIdentifier, prefs: &VoiceDescr) -> Result<&'static str, TtsConstructionError> {
        let lang_str = format!("{}-{}", lang.language, lang.region.ok_or(TtsConstructionError::NoRegion)?.as_str());
        match lang_str.as_str() {
            "es-ES" => {
                Ok(match prefs.gender {
                    Gender::Male => "es-ES_EnriqueV3Voice",
                    Gender::Female => "es-ES_LauraV3Voice"
                })
            }
            "en-US" => {
                Ok(match prefs.gender {
                    Gender::Male => "en-US_MichaelV3Voice",
                    Gender::Female =>  "en-US_AllisonV3Voice"
                })
            }
            _ => Err(TtsConstructionError::IncompatibleLanguage)
        }
    }

    // Accept only negotiated LanguageIdentifiers
    fn lang_neg(lang: &LanguageIdentifier) -> LanguageIdentifier {
        let default = langid!("en-US");
        negotiate_langs_res(lang, &Self::available_langs(), Some(&default)).expect(NO_COMPATIBLE_LANG_MSG)
    }

    fn available_langs() -> Vec<LanguageIdentifier> {
        langids!("es-ES", "en-US")
    }
}

impl Tts for IbmTts {
    fn synth_text(&mut self, input: &str) -> Result<Audio, TtsError> {
        Ok(self.engine.synth(input, &self.curr_voice).map(|b|Audio::new_encoded(b, DEFAULT_SAMPLES_PER_SECOND))?)
    }

    fn get_info(&self) -> TtsInfo {
        TtsInfo {
            name: "IBM Text To Speech".to_string(),
            is_online: true
        }
    }
}


impl TtsStatic for IbmTts {
    fn is_descr_compatible(_descr: &VoiceDescr) -> Result<(),TtsConstructionError> {
        // Ibm has voices for both genders in all supported languages
        Ok(())
    }

    fn is_lang_comptaible(lang: &LanguageIdentifier) -> Result<(), TtsConstructionError> {
        negotiate_langs_res(lang, &Self::available_langs(), None).map(|_|())
    }
}

pub struct IbmTtsEngine {
	client: reqwest::blocking::Client,
	api_gateway: String,
	api_key: String
}

impl IbmTtsEngine {

	pub fn new(api_gateway: String, api_key: String) -> Self {
		IbmTtsEngine{client: reqwest::blocking::Client::new(), api_gateway, api_key}
	}

	pub fn synth(&mut self, text: &str, voice: &str) -> Result<Vec<u8>, OnlineTtsError> {
	    let url_str = format!("https://{}/text-to-speech/api/v1/synthesize?voice=", self.api_gateway);
	    let url = reqwest::Url::parse(&format!("{}{}&text={}", url_str, voice, text))?;

		let mut buf: Vec<u8> = vec![];
	    self.client.post(url).header("accept", "audio/mp3").header("Authorization",format!("Basic {}",base64::encode(&format!("apikey:{}", self.api_key)))).send()?.copy_to(&mut buf)?;

		Ok(buf)
	}
}
