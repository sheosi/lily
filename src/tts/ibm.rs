use crate::tts::{Tts, VoiceDescr, TtsConstructionError, Gender, TtsError, TtsInfo, TtsStatic, OnlineTtsError, negotiate_langs_res};
use crate::vars::{NO_COMPATIBLE_LANG_MSG, DEFAULT_SAMPLES_PER_SECOND};

use lily_common::audio::Audio;
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
            "ar-AR" => {
                Ok("ar-AR_OmarVoice") // Only male
            }
            "de-DE" => {
                Ok(match prefs.gender{ // There's also "de-DE_ErikaV3Voice"
                    Gender::Male => "de-DE_DieterV3Voice",
                    Gender::Female => "de-DE_BirgitV3Voice"
                })
            }
            "en-GB" => {
                Ok(match prefs.gender { // There's also "en-GB_KateV3Voice"
                    Gender::Male => "en-GB_JamesV3Voice",
                    Gender::Female => "en-GB_CharlotteV3Voice"
                })
            }
            "en-US" => {
                Ok(match prefs.gender { // There's also "en-US_EmilyV3Voice", "en-US_HenryV3Voice", "en-US_KevinV3Voice", "en-US_LisaV3Voice" and  "en-US_OliviaV3Voice"
                    Gender::Male => "en-US_MichaelV3Voice",
                    Gender::Female =>  "en-US_AllisonV3Voice"
                })
            }
            "es-ES" => {
                Ok(match prefs.gender {
                    Gender::Male => "es-ES_EnriqueV3Voice",
                    Gender::Female => "es-ES_LauraV3Voice"
                })
            }
            "es-LA" => {
                Ok("es-LA_SofiaV3Voice") // Only female
            }
            "es-US" => {
                Ok("es-US_SofiaV3Voice") // Only female
            }
            "fr-FR" => {
                Ok(match prefs.gender {
                    Gender::Male => "fr-FR_NicolasV3Voice",
                    Gender::Female => "fr-FR_ReneeV3Voice"
                })
            }
            "it-IT" => {
                Ok("it-IT_FrancescaV3Voice") // Only female
            }
            "ja-JP" => {
                Ok("ja-JP_EmiVoice") // Only female
            }
            "ko-KR" => {
                Ok("ko-KR_YoungmiVoice") // There's also "ko-KR_YunaVoice"
            }
            "nl-NL" => {
                Ok(match prefs.gender {
                    Gender::Male => "nl-NL_LiamVoice",
                    Gender::Female => "nl-NL_EmmaVoice"
                })
            }
            "pt-BR" => {
                Ok("pt-BR_IsabelaV3Voice") // Only female
            }
            "zh-CN" => {
                Ok(match prefs.gender { // There's also "zh-CN_ZhangJingVoice"
                    Gender::Male => "zh-CN_WangWeiVoice",
                    Gender::Female => "zh-CN_LiNaVoice"
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
        langids!("ar-AR", "de-DE", "en-GB", "en-US", "es-ES", "es-LA", "es-US",
                 "fr-FR", "it-IT", "ja-JP", "ko-KR", "nl-NL", "pt-BR", "zh-CN")
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
