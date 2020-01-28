// Other crates
use serde::Deserialize;
use thiserror::Error;

#[derive(Deserialize)]
struct WattsonResponse {

    results: Vec<WattsonResult>,
    result_index: u8
}

#[derive(Deserialize)]
struct WattsonResult {
	alternatives: Vec<WattsonAlternative>,
	//r#final: bool
}

#[derive(Deserialize)]
struct WattsonAlternative {
	//confidence: f32,
	transcript: String
}

#[cfg(feature = "google_tts")]
pub struct GttsEngine {
	client: reqwest::blocking::Client
}

#[cfg(feature = "google_tts")]
impl GttsEngine {
	pub fn new() -> Self {
		GttsEngine{client: reqwest::blocking::Client::new()}
	}

	// This one will return an MP3
	pub fn synth(&mut self, text: &str, lang: &str) -> Result<Vec<u8>, GttsError> {
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

pub struct IbmSttEngine {
	client: reqwest::blocking::Client,
	api_gateway: String,
	api_key: String
}

impl IbmSttEngine {

	pub fn new(api_gateway: String, api_key: String) -> Self {
		IbmSttEngine{client: reqwest::blocking::Client::new(), api_gateway, api_key}
	}

	// This one will return a wav
	pub fn decode(&mut self, audio: &crate::audio::AudioRaw, model: &str) -> Result<Option<(String, Option<String>, i32)>, GttsError> {
	    let url_str = format!("https://{}/speech-to-text/api/v1/recognize?model=", self.api_gateway);
	    let url = reqwest::Url::parse(&format!("{}{}", url_str, model))?; 
	    let as_wav = audio.to_wav()?;
	    
	    let res = self.client.post(url).body(as_wav).header("Content-Type", "audio/wav").header("Authorization",format!("Basic {}",base64::encode(&format!("apikey:{}", self.api_key)))).send()?.text()?;
	    log::info!("{}", res);
	    let response: WattsonResponse = serde_json::from_str(&res)?;
	    let res = {
	    	if !response.results.is_empty() {
		    	let alternatives = &response.results[response.result_index as usize].alternatives;

		    	if !alternatives.is_empty() {
		    		let res_str = &alternatives[0].transcript;
		    		Some((res_str.to_string() , None, 0))
		    	}
		    	else {
		    		None
		    	}
	    	}
	    	else {
	    		None
	    	}
	    };

	    Ok(res)
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

	pub fn synth(&mut self, text: &str, voice: &str) -> Result<Vec<u8>, reqwest::Error> {
	    let url_str = format!("https://{}/text-to-speech/api/v1/synthesize?voice=", self.api_gateway);
	    let url = reqwest::Url::parse(&format!("{}{}&text={}", url_str, voice, text)).unwrap();

		let mut buf: Vec<u8> = vec![];
	    self.client.post(url).header("accept", "audio/mp3").header("Authorization",format!("Basic {}",base64::encode(&format!("apikey:{}", self.api_key)))).send().unwrap().copy_to(&mut buf)?;

		Ok(buf)
	}
}


#[derive(Error,Debug)]
pub enum GttsError {
	#[error("network failure")]
	Network(#[from] reqwest::Error),

	#[error("url parsing")]
	UrlParse(#[from] url::ParseError),

	#[error("wav conversion")]
	WavConvert(#[from] crate::audio::WavError),

	#[error("json parsing")]
	JsonParse(#[from] serde_json::Error)
}
