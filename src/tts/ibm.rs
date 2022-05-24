use crate::tts::{Gender, TtsConstructionError, TtsError, TtsInfo, VoiceDescr};

use crate::tts::http_tts::HttpsTtsData;

use reqwest::{RequestBuilder, Url};
use serde::Deserialize;
use unic_langid::{langid, langids, LanguageIdentifier};

#[derive(Clone, Debug, Deserialize)]
pub struct IbmTtsData {
    pub key: String,
    pub gateway: String,
}

impl HttpsTtsData for IbmTtsData {
    fn make_request_url(&self, voice: &str, input: &str) -> Result<Url, TtsError> {
        let url_str = format!("https://{}/text-to-speech/api/v1/synthesize", self.gateway);

        Ok(Url::parse_with_params(&url_str, &[("voice", voice), ("text", input)]).unwrap())
    }

    fn edit_request(&self, _input: &str, req: RequestBuilder) -> RequestBuilder {
        req.header(
            "Authorization",
            format!("Basic {}", base64::encode(&format!("apikey:{}", self.key))),
        )
    }

    fn get_voice_name(
        &self,
        lang: &str,
        region: &str,
        prefs: &VoiceDescr,
    ) -> Result<String, TtsConstructionError> {
        let res = match (lang, region, &prefs.gender) {
            ("ar", _, _) => Ok("ar-AR_OmarVoice"), // Only male
            ("de", _, Gender::Male) => Ok("de-DE_DieterV3Voice"),
            ("de", _, Gender::Female) => Ok("de-DE_BirgitV3Voice"), // There's also "de-DE_ErikaV3Voice"
            ("en", "GB", Gender::Male) => Ok("en-GB_JamesV3Voice"),
            ("en", "GB", Gender::Female) => Ok("en-GB_CharlotteV3Voice"), // There's also "en-GB_KateV3Voice"
            ("en", "US", Gender::Male) => Ok("en-US_MichaelV3Voice"), // There's alseo "en-US_HenryV3Voice" and "en-US_KevinV3Voice"
            ("en", "US", Gender::Female) => Ok("en-US_AllisonV3Voice"), // There's also "en-US_EmilyV3Voice", "en-US_LisaV3Voice" and  "en-US_OliviaV3Voice"
            ("es", "ES", Gender::Male) => Ok("es-ES_EnriqueV3Voice"),
            ("es", "ES", Gender::Female) => Ok("es-ES_LauraV3Voice"),
            ("es", "LA", _) => Ok("es-LA_SofiaV3Voice"), // Only female
            ("es", "US", _) => Ok("es-US_SofiaV3Voice"), // Only female
            ("fr", _, Gender::Male) => Ok("fr-FR_NicolasV3Voice"),
            ("fr", _, Gender::Female) => Ok("fr-FR_ReneeV3Voice"),
            ("it", _, _) => Ok("it-IT_FrancescaV3Voice"), // Only female
            ("ja", _, _) => Ok("ja-JP_EmiVoice"),         // Only female
            ("ko", _, _) => Ok("ko-KR_YoungmiVoice"),     // There's also "ko-KR_YunaVoice"
            ("nl", _, Gender::Male) => Ok("nl-NL_LiamVoice"),
            ("nl", _, Gender::Female) => Ok("nl-NL_EmmaVoice"),
            ("pt", _, _) => Ok("pt-BR_IsabelaV3Voice"), // Only female
            ("zh", _, Gender::Male) => Ok("zh-CN_WangWeiVoice"), // There's also "zh-CN_ZhangJingVoice"
            ("zh", _, Gender::Female) => Ok("zh-CN_LiNaVoice"),
            _ => Err(TtsConstructionError::IncompatibleLanguage),
        };

        res.map(|s| s.into())
    }

    fn get_info(&self) -> TtsInfo {
        TtsInfo {
            name: "IBM Text To Speech".to_string(),
            is_online: true,
        }
    }

    fn get_available_langs(&self) -> Vec<LanguageIdentifier> {
        langids!(
            "ar-AR", "de-DE", "en-GB", "en-US", "es-ES", "es-LA", "es-US", "fr-FR", "it-IT",
            "ja-JP", "ko-KR", "nl-NL", "pt-BR", "zh-CN"
        )
    }
}
