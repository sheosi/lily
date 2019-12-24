// Other crates
use rodio::source::Source;
use serde::Deserialize;

// Optional dependencies
#[cfg(feature = "google_tts")]
use std::io::Write;

#[derive(Deserialize)]
struct WattsonResponse {

    results: Vec<WattsonResult>,
    result_index: u8
}

#[derive(Deserialize)]
struct WattsonResult {
	alternatives: Vec<WattsonAlternative>,
	r#final: bool
}

#[derive(Deserialize)]
struct WattsonAlternative {
	confidence: f32,
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

	pub fn synth(&mut self, text: &str, lang: &str) -> Result<(), reqwest::Error> {
		const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; WOW64) \
	            AppleWebKit/537.36 (KHTML, like Gecko) \
	            Chrome/47.0.2526.106 Safari/537.36";

	    let url = google_translate_tts::url(text, lang);
	    log::info!("{}", url);

	    let mut buf: Vec<u8> = vec![];
	    self.client.get(&url).header("Referer", "http://translate.google.com/").header("User-Agent", USER_AGENT).send()?
	    .copy_to(&mut buf)?;

        let mut file = std::fs::File::create("translate_tts.mp3").unwrap();
        // Write a slice of bytes to the file
        file.write_all(&buf).unwrap();

		let device = rodio::default_output_device().unwrap();
		//let source = rodio::Decoder::new(std::io::Cursor::new(buf)).unwrap();
		let source = rodio::Decoder::new(std::io::BufReader::new(std::fs::File::open("translate_tts.mp3").unwrap())).unwrap();
		rodio::play_raw(&device, source.convert_samples());

		Ok(())
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

	pub fn decode(&mut self, audio: &crate::audio::Audio, model: &str) -> Result<Option<(String, Option<String>, i32)>, reqwest::Error> {
	    let url_str = format!("https://{}/speech-to-text/api/v1/recognize?model=", self.api_gateway);
	    let url = reqwest::Url::parse(&format!("{}{}", url_str, model)).unwrap();
	    //audio.write_wav("temp_stt.wav").unwrap();
	    //std::process::Command::new("sox").arg("temp_stt.wav").arg("temp_stt.flac").spawn().expect("sox failed").wait().expect("sox failed 2");
	    let as_wav = audio.to_wav();

	    //let file = std::fs::File::open("temp_stt.flac").unwrap();
	    //let res = self.client.post(url).body(file).header("Content-Type", "audio/wav").header("Authorization",format!("Basic {}",base64::encode(&format!("apikey:{}", self.api_key)))).send()?.text()?;
	    let res = self.client.post(url).body(as_wav).header("Content-Type", "audio/wav").header("Authorization",format!("Basic {}",base64::encode(&format!("apikey:{}", self.api_key)))).send()?.text()?;
	    log::info!("{}", res);
	    let response: WattsonResponse = serde_json::from_str(&res).unwrap();
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

	pub fn synth(&mut self, text: &str, voice: &str) -> Result<(), reqwest::Error> {
	    let url_str = format!("https://{}/text-to-speech/api/v1/synthesize?voice=", self.api_gateway);
	    let url = reqwest::Url::parse(&format!("{}{}&text={}", url_str, voice, text)).unwrap();

		let mut buf: Vec<u8> = vec![];
	    self.client.post(url).header("accept", "audio/mp3").header("Authorization",format!("Basic {}",base64::encode(&format!("apikey:{}", self.api_key)))).send().unwrap().copy_to(&mut buf)?;
	    //let response: WattsonResponse = serde_json::from_str(&res).unwrap();
	    //let res_str = &response.results[response.result_index as usize].alternatives[0].transcript;

		let device = rodio::default_output_device().unwrap();
		let source = rodio::Decoder::new(std::io::Cursor::new(buf)).unwrap();
		//let source = rodio::Decoder::new(std::io::BufReader::new(std::fs::File::open("translate_tts.mp3").unwrap())).unwrap();
		rodio::play_raw(&device, source.convert_samples());

		Ok(())
	}
}