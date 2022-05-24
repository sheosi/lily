use crate::tts::{TtsConstructionError, TtsError, TtsInfo, VoiceDescr};

use reqwest::{RequestBuilder, Url};
use unic_langid::{langid, langids, LanguageIdentifier};

use super::http_tts::HttpsTtsData;

pub struct GttsData();

impl HttpsTtsData for GttsData {
    fn make_request_url(&self, voice: &str, input: &str) -> Result<reqwest::Url, TtsError> {
        Ok(Url::parse(&google_translate_tts::url(input, voice)).unwrap())
    }

    fn edit_request(&self, input: &str, req: reqwest::RequestBuilder) -> RequestBuilder {
        const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; WOW64) \
	            AppleWebKit/537.36 (KHTML, like Gecko) \
	            Chrome/47.0.2526.106 Safari/537.36";

        req.header("Referer", "http://translate.google.com/")
            .header("User-Agent", USER_AGENT)
    }

    fn get_voice_name(
        &self,
        lang: &str,
        region: &str,
        prefs: &VoiceDescr,
    ) -> Result<String, TtsConstructionError> {
        Ok(format!("{}-{}", lang, region))
    }

    fn get_info(&self) -> TtsInfo {
        TtsInfo {
            name: "Google Translate".to_string(),
            is_online: true,
        }
    }

    fn get_available_langs(&self) -> Vec<LanguageIdentifier> {
        // Note: Google also allows others ("sq", "hy", "bs", "hr", "eo", "mk",
        // "sw", "cy"), however, they use what seems to be Espeak, at which point
        // you are better off just using espeak yourself
        langids!(
            "af", "ar", "bn", "bg", "ca", "cs", "da", "de", "en", "et", "el", "fi", "fr", "gu",
            "he", "hi", "hu", "is", "id", "it", "iw", "ja", "jv", "kn", "km", "ko", "nl", "lv",
            "ms", "ml", "my", "ne", "no", "pl", "pt", "ro", "ru", "sr", "si", "sk", "es", "su",
            "sv", "tl", "ta", "te", "th", "tr", "uk", "ur", "vi", "zh-CN", "zh-TW"
        )
    }
}
