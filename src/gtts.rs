use crate::lang::Lang;
use rodio::source::Source;
use std::io::Write;
use serde::Deserialize;

const IBM_API_KEY: &str = "";
const IBM_API_GATEWAY: &str = "";

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

pub struct GttsEngine {
	client: reqwest::blocking::Client
}


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
	client: reqwest::blocking::Client
}

impl IbmSttEngine {

	pub fn new() -> Self {
		IbmSttEngine{client: reqwest::blocking::Client::new(), }
	}

	pub fn decode(&mut self, audio: &crate::audio::Audio, model: &str) -> Result<(String, Option<String>, i32), reqwest::Error> {
	    let url_str = format!("https://{}/speech-to-text/api/v1/recognize?model=", IBM_API_GATEWAY);
	    let url = reqwest::Url::parse(&format!("{}{}", url_str, model)).unwrap();
	    audio.write_wav("temp_stt.wav");

	    std::process::Command::new("sox").arg("temp_stt.wav").arg("temp_stt.flac").spawn().expect("sox failed").wait().expect("sox failed 2");

	    let file = std::fs::File::open("temp_stt.flac").unwrap();
	    let res = self.client.post(url).body(file).header("Content-Type", "audio/flac").header("Authorization",format!("Basic {}",base64::encode(&format!("apikey:{}", IBM_API_KEY)))).send()?.text()?;
	    log::info!("{}", res);
	    let response: WattsonResponse = serde_json::from_str(&res).unwrap();
	    let res_str = &response.results[response.result_index as usize].alternatives[0].transcript;
	    println!("Wattson: {}", &res_str);

	    Ok((res_str.to_string() , None, 0))
	}
}

pub struct IbmTtsEngine {
	client: reqwest::blocking::Client
}

impl IbmTtsEngine {

	pub fn new() -> Self {
		IbmTtsEngine{client: reqwest::blocking::Client::new()}
	}

	pub fn synth(&mut self, text: &str, voice: &str) -> Result<(), reqwest::Error> {
	    let url_str = format!("https://{}/text-to-speech/api/v1/synthesize?voice=", IBM_API_GATEWAY);
	    let url = reqwest::Url::parse(&format!("{}{}&text={}", url_str, voice, text)).unwrap();

		let mut buf: Vec<u8> = vec![];
	    self.client.post(url).header("Authorization",format!("Basic {}",base64::encode(&format!("apikey:{}", IBM_API_KEY)))).send().unwrap().copy_to(&mut buf)?;
	    //let response: WattsonResponse = serde_json::from_str(&res).unwrap();
	    //let res_str = &response.results[response.result_index as usize].alternatives[0].transcript;

		let device = rodio::default_output_device().unwrap();
		let source = rodio::Decoder::new(std::io::Cursor::new(buf)).unwrap();
		//let source = rodio::Decoder::new(std::io::BufReader::new(std::fs::File::open("translate_tts.mp3").unwrap())).unwrap();
		rodio::play_raw(&device, source.convert_samples());

		Ok(())
	}
}