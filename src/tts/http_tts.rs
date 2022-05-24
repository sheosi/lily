use crate::tts::{
    negotiate_langs_res, Tts, TtsConstructionError, TtsError, TtsInfo,
    TtsStatic, VoiceDescr,
};

use async_trait::async_trait;
use lily_common::audio::Audio;
use reqwest::{Client, RequestBuilder, Url};
use unic_langid::LanguageIdentifier;

pub struct HttpTts<H: HttpsTtsData> {
    d: H,
    client: Client,
    curr_voice: String,
}

pub trait HttpsTtsData {
    fn make_request_url(&self, voice: &str, input: &str) -> Result<Url, TtsError>;
    fn edit_request(&self, input: &str, req: RequestBuilder) -> RequestBuilder;
    fn get_voice_name(
        &self,
        lang: &str,
        region: &str,
        prefs: &VoiceDescr,
    ) -> Result<String, TtsConstructionError>;
    fn get_info(&self) -> TtsInfo;

    fn get_available_langs(&self) -> Vec<LanguageIdentifier>;
}

impl<H: HttpsTtsData> HttpTts<H> {
    pub fn new(
        lang: &LanguageIdentifier,
        prefs: &VoiceDescr,
        d: H,
    ) -> Result<Self, TtsConstructionError> {
        Ok(Self {
            client: Client::new(),
            curr_voice: Self::make_tts_voice(&d, lang, prefs)?,
            d,
        })
    }

    fn make_tts_voice(d: &H,
        lang: &LanguageIdentifier,
        prefs: &VoiceDescr,
    ) -> Result<String, TtsConstructionError> {
        d.get_voice_name(
            lang.language.as_str(),
            lang.region.ok_or(TtsConstructionError::NoRegion)?.as_str(),
            prefs,
        )
    }
}

#[async_trait(?Send)]
impl<H: HttpsTtsData> Tts for HttpTts<H> {
    async fn synth_text(&mut self, input: &str) -> Result<Audio, TtsError> {
        let audio = self
            .d
            .edit_request(
                input,
                self.client
                    .get(self.d.make_request_url(&self.curr_voice, input)?)
                    .header("accept", "audio/mp3"),
            )
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap()
            .to_vec();

        Ok(Audio::new_encoded(audio))
    }

    fn get_info(&self) -> TtsInfo {
        self.d.get_info()
    }
}

impl<H: HttpsTtsData> TtsStatic for HttpTts<H> {
    type Data = H;
    fn is_descr_compatible(_d: &Self::Data, _descr: &VoiceDescr) -> Result<(), TtsConstructionError> {
        // Ibm has voices for both genders in all supported languages
        Ok(())
    }

    fn is_lang_comptaible(d: &Self::Data, lang: &LanguageIdentifier) -> Result<(), TtsConstructionError> {
        negotiate_langs_res(lang, &d.get_available_langs(), None).map(|_| ())
    }
}