use crate::audio::AudioRaw;
use crate::stt::{SttInfo, SttConstructionError, SttBatched, OnlineSttError,SttError, SttVadless};
use crate::vars::DEFAULT_SAMPLES_PER_SECOND;

use fluent_langneg::{negotiate_languages, NegotiationStrategy};
use reqwest::blocking;
use serde::Deserialize;
use unic_langid::{LanguageIdentifier, langid, langids};
pub struct IbmStt {
    engine: IbmSttEngine,
    model: String
}

impl IbmStt {

    pub fn new(lang: &LanguageIdentifier, api_gateway: String, api_key: String) -> Result<Self, SttConstructionError> {
        Ok(IbmStt{engine: IbmSttEngine::new(api_gateway, api_key), model: Self::model_from_lang(lang)?.to_string()})
    }

    fn model_from_lang(lang: &LanguageIdentifier) -> Result<String, SttConstructionError> {
        let lang = Self::lang_neg(lang);
        Ok(format!("{}-{}_BroadbandModel", lang.language, lang.region.ok_or(SttConstructionError::NoRegion)?))
    }

    fn lang_neg(lang: &LanguageIdentifier) -> LanguageIdentifier {
        let available_langs = langids!("es-ES", "en-US");
        let default = langid!("en-US");
        negotiate_languages(&[lang],&available_langs, Some(&default), NegotiationStrategy::Filtering)[0].clone()
    }
}

impl SttBatched for IbmStt {
    
    fn decode(&mut self, audio: &[i16]) -> Result<Option<(String, Option<String>, i32)>, SttError> {
        Ok(self.engine.decode(&AudioRaw::new_raw(audio.to_vec(), DEFAULT_SAMPLES_PER_SECOND), &self.model)?)
    }

    fn get_info(&self) -> SttInfo {
        SttInfo {name: "Ibm's Speech To Text".to_string(), is_online: true}
    }
}

impl SttVadless for IbmStt {
    fn process(&mut self, audio: &[i16]) -> Result<(), SttError> {
        self.engine.live_process(&AudioRaw::new_raw(audio.to_vec(), 16000), &self.model)?;
        Ok(())
    }
    fn end_decoding(&mut self) -> Result<Option<(String, Option<String>, i32)>, SttError> {
        println!("End decode ");
        let res = self.engine.live_process_end(&self.model)?;
        Ok(res)
    }
    fn get_info(&self) -> SttInfo {
        SttInfo {name: "Ibm's Speech To Text".to_string(), is_online: true}
    }
}

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


pub struct IbmSttEngine {
	client: blocking::Client,
	api_gateway: String,
	api_key: String
}

impl IbmSttEngine {

	pub fn new(api_gateway: String, api_key: String) -> Self {
		IbmSttEngine{client: blocking::Client::new(), api_gateway, api_key}
	}

	// Send all audio in one big chunk
	pub fn decode(&mut self, audio: &AudioRaw, model: &str) -> Result<Option<(String, Option<String>, i32)>, OnlineSttError> {
	    let url_str = format!("https://{}/speech-to-text/api/v1/recognize?model={}", self.api_gateway, model);
	    println!("{}", url_str);
	    let url = reqwest::Url::parse(&url_str)?; 

	    let as_ogg = audio.to_ogg_opus()?;	    
	    let res = self.client.post(url).body(as_ogg).header("Content-Type", "audio/ogg").header("Authorization",format!("Basic {}",base64::encode(&format!("apikey:{}", self.api_key)))).send()?.text()?;
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

	pub fn live_process(&mut self, audio: &crate::audio::AudioRaw, model: &str) -> Result<(), OnlineSttError> {
		let url_str = format!("https://{}/speech-to-text/api/v1/recognize?model=", self.api_gateway);
	    let url = reqwest::Url::parse(&format!("{}{}", url_str, model))?; 

	    let as_ogg = audio.to_ogg_opus()?;
	    //as_ogg.push(b'\r');
	    //as_ogg.push(b'\n');

	    
	    log::info!("Ogg len: {}",as_ogg.len());
	    //let mut file = std::fs::File::create("test.ogg").unwrap();
	    //file.write_all(&as_ogg).unwrap();

	    /*let mut len_str = as_ogg.len().to_string().into_bytes();
	    len_str.push(b'\r');
	    len_str.push(b'\n');*/
	    

	    //len_str.extend(as_ogg);
	    //let res = self.client.post(url.clone()).body(len_str).header(reqwest::header::CONTENT_TYPE, "audio/wav").header(reqwest::header::TRANSFER_ENCODING, "chunked").header(reqwest::header::AUTHORIZATION,format!("Basic {}",base64::encode(&format!("apikey:{}", self.api_key)))).send()?.text()?;
	    //log::info!("{}", res);
	    
	    
	    let res = self.client.post(url).body(as_ogg).header(reqwest::header::CONTENT_TYPE, "audio/ogg").header(reqwest::header::TRANSFER_ENCODING, "chunked").header(reqwest::header::AUTHORIZATION,format!("Basic {}",base64::encode(&format!("apikey:{}", self.api_key)))).send()?.text()?;
	    log::info!("{}", res);
	    //let response: WattsonResponse = serde_json::from_str(&res)?;

	    Ok(/*res*/())
	}
	pub fn live_process_end(&mut self, model: &str) -> Result<Option<(String, Option<String>, i32)>, OnlineSttError> {
		let url_str = format!("https://{}/speech-to-text/api/v1/recognize?model=", self.api_gateway);
	    let url = reqwest::Url::parse(&format!("{}{}", url_str, model))?; 


	    let res = self.client.post(url.clone()).body("0\r\n").header(reqwest::header::CONTENT_TYPE, "audio/ogg").header(reqwest::header::TRANSFER_ENCODING, "chunked").header(reqwest::header::AUTHORIZATION,format!("Basic {}",base64::encode(&format!("apikey:{}", self.api_key)))).send()?.text()?;
	    log::info!("{}", res);
	    
	    // let mut buffer = crate::audio::AudioWav::empty_wav(16000)?;
	   	// buffer.push(b'\r');
	    // buffer.push(b'\n');

	    // let res = self.client.post(url).body(buffer).header(reqwest::header::CONTENT_TYPE, "audio/wav").header(reqwest::header::TRANSFER_ENCODING, "chunked").header(reqwest::header::AUTHORIZATION,format!("Basic {}",base64::encode(&format!("apikey:{}", self.api_key)))).send()?.text()?;
	    // log::info!("{}", res);
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

