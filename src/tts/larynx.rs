use crate::tts::{TtsConstructionError, TtsError, TtsInfo, VoiceDescr};

use regex::Regex;
use reqwest::{Client, Url};
use serde::Deserialize;
use unic_langid::LanguageIdentifier;

use super::http_tts::HttpsTtsData;

#[derive(Clone, Debug, Deserialize)]
pub struct LarynxData {
    #[serde(flatten)]
    url: String,

    #[serde(skip_deserializing, default)]
    voices: Vec<String>,
}

impl HttpsTtsData for LarynxData {
    fn make_request_url(&self, voice: &str, _input: &str) -> Result<Url, TtsError> {
        let mut url = Url::parse(&self.url).unwrap().join("/api/tts").unwrap();
        url.set_query(Some(&format!("voice={}", voice)));
        Ok(url)
    }

    fn edit_request(&self, input: &str, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        req.header("Content-type", "text/plain")
            .body(input.to_string())
    }

    fn get_voice_name(
        &self,
        lang: &str,
        region: &str,
        _prefs: &VoiceDescr,
    ) -> Result<String, TtsConstructionError> {
        let l = format!("{}-{}/", lang, region);

        for v in &self.voices {
            if v.starts_with(&l) {
                return Ok(v.clone());
            }
        }

        Err(TtsConstructionError::IncompatibleLanguage)
    }

    fn get_info(&self) -> TtsInfo {
        TtsInfo {
            name: "Larynx".into(),
            is_online: true,
        }
    }

    fn get_available_langs(&self) -> Vec<LanguageIdentifier> {
        let lang_reg =
            Regex::new("^([a-zA-Z]+-[a-zA-Z]+)/").expect("Regex failed to compile, report this");

        self.voices
            .iter()
            .filter_map(|s| lang_reg.captures(s))
            .filter_map(|c| {
                c.get(1)
                    .expect("Failed to extract capture, report this")
                    .as_str()
                    .parse()
                    .ok()
            })
            .collect::<Vec<LanguageIdentifier>>()
    }
}

impl LarynxData {
    async fn obtain_voices(url: &str) -> Vec<String> {
        let c = Client::new();
        let voices_bytes = c
            .get(Url::parse(url).unwrap().join("/api/voices").unwrap())
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();

        std::str::from_utf8(&voices_bytes)
            .unwrap()
            .split(',')
            .map(|s| s.to_string())
            .collect()
    }
}
