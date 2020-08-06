use std::time::{Duration, Instant};

use crate::audio::AudioRaw;
use crate::stt::{DecodeRes, OnlineSttError, SttInfo, SttConstructionError, SttBatched,SttError, SttVadless};
use crate::vars::DEFAULT_SAMPLES_PER_SECOND;

use fluent_langneg::{negotiate_languages, NegotiationStrategy};
use maplit::hashmap;
use reqwest::{blocking, header};
use tungstenite::{client::AutoStream, connect, Message, WebSocket};
use serde::{Deserialize, Serialize};
use url::Url;
use unic_langid::{LanguageIdentifier, langid, langids};
use log::warn;

pub struct IbmStt {
    engine: IbmSttEngine,
    model: String
}

#[derive(Deserialize, Debug, Clone)]
pub struct IbmSttData {
	api_key: String,
	instance: String,
	gateway: String
}

impl IbmStt {

    pub fn new(lang: &LanguageIdentifier, data: IbmSttData) -> Result<Self, SttConstructionError> {
        Ok(IbmStt{engine: IbmSttEngine::new(data), model: Self::model_from_lang(lang)?.to_string()})
    }

    fn model_from_lang(lang: &LanguageIdentifier) -> Result<String, SttConstructionError> {
        let lang = Self::lang_neg(lang);
        Ok(format!("{}-{}_BroadbandModel", lang.language, lang.region.ok_or(SttConstructionError::NoRegion)?))
    }

    fn lang_neg(lang: &LanguageIdentifier) -> LanguageIdentifier {
        let available_langs = langids!(
			"es-ES", "en-US"
		);

        let default = langid!("en-US");
        negotiate_languages(&[lang],&available_langs, Some(&default), NegotiationStrategy::Filtering)[0].clone()
    }
}

impl SttBatched for IbmStt {
    
    fn decode(&mut self, audio: &[i16]) -> Result<Option<DecodeRes>, SttError> {
        Ok(self.engine.decode(&AudioRaw::new_raw(audio.to_vec(), DEFAULT_SAMPLES_PER_SECOND), &self.model)?)
    }

    fn get_info(&self) -> SttInfo {
        SttInfo {name: "Ibm's Speech To Text".to_string(), is_online: true}
    }
}

impl SttVadless for IbmStt {
	fn begin_decoding(&mut self) -> Result<(), SttError> {
		self.engine.live_process_begin(&self.model)?;
		Ok(())
	}
    fn process(&mut self, audio: &[i16]) -> Result<(), SttError> {
        self.engine.live_process(&AudioRaw::new_raw(audio.to_vec(), 16000))?;
        Ok(())
    }
    fn end_decoding(&mut self) -> Result<Option<DecodeRes>, SttError> {
        let res = self.engine.live_process_end()?;
        Ok(res)
    }
    fn get_info(&self) -> SttInfo {
        SttInfo {name: "Ibm's Speech To Text".to_string(), is_online: true}
    }
}

#[derive(Deserialize)]
struct WatsonResponse {
    results: Vec<WatsonResult>,
    result_index: u8
}

#[derive(Deserialize)]
struct WatsonResult {
	alternatives: Vec<WatsonAlternative>,
	//r#final: bool
}

#[derive(Deserialize)]
struct WatsonAlternative {
	//confidence: f32,
	transcript: String
}


pub struct IbmSttEngine {
	curr_socket: Option <WatsonSocket>,
	data: IbmSttData,
	token_cache: TokenCache
}

struct WatsonSocket {
	socket: WebSocket<AutoStream>
}

enum WatsonOrder {
	Start,
	Stop
}

impl WatsonSocket {

	fn new(model: &str, data: IbmSttData, token: &str) -> Result<Self, OnlineSttError> {
		let url_str = format!("wss://api.{}.speech-to-text.watson.cloud.ibm.com/instances/{}/v1/recognize", data.gateway, data.instance);
		let (socket, _response) =
			connect(Url::parse_with_params(&url_str,&[
				("access_token", token),
				("model", model)
			])?)?;
		Ok(Self{socket})
	}

	fn send_order(&mut self, order: WatsonOrder) -> Result<(), OnlineSttError> {
		#[derive(Serialize)]
		struct WatsonOrderInternal<'a> {
			action: &'a str,
			#[serde(skip_serializing_if="Option::is_none")]
			#[serde(rename="content-type")]
			content_type: Option<&'a str>
		}

	
		let order = match order {
			WatsonOrder::Start => WatsonOrderInternal{action: "start", content_type: Some("audio/ogg")},
			WatsonOrder::Stop => WatsonOrderInternal{action: "stop", content_type: None}
		};
		let order_str = serde_json::to_string(&order)?;
		self.socket
		.write_message(Message::Text(order_str))?;

		Ok(())
	}

	fn send_audio(&mut self, audio: &AudioRaw) -> Result<(), OnlineSttError> {
		let as_ogg = audio.to_ogg_opus()?;
		self.socket
		.write_message(Message::Binary(as_ogg))?;

		Ok(())
	}

	fn get_answer(&mut self) -> Result<Option<DecodeRes>, OnlineSttError> {
		// Ignore {"state": "listening"}
		loop {
			if let Message::Text(response_str) = self.socket.read_message().expect("Error reading message") {
				let response_res: Result<WatsonResponse,_> = serde_json::from_str(&response_str);
				if let Ok(response) = response_res {
					let res = {
						if !response.results.is_empty() {
							let alternatives = &response.results[response.result_index as usize].alternatives;

							if !alternatives.is_empty() {
								let res_str = &alternatives[0].transcript;
								Some(DecodeRes{hypothesis: res_str.to_string()})
							}
							else {
								None
							}
						}
						else {
							None
						}
					};
					return Ok(res)
				}
			}
		}
	
	}

	fn close(mut self) -> Result<(), OnlineSttError> {
		self.socket.close(None)?;
		// TODO: Make sure we get Error::ConnectionClosed

		Ok(())
	}
}

struct TokenCache {
	data: Option<(String, Instant)>,
}

impl TokenCache {
	fn new(api_key: &str) -> Self {
		let mut res = TokenCache{data: None};
		if let Err(err) = res.get(api_key) {
			warn!("Initial IBM API key couldn't be obtained, continuing regardless: {:?}", err);
		}

		res
	}

	fn gen_iam_token(api_key: &str) -> Result<(String, u16), OnlineSttError> {
		#[derive(Debug, Deserialize)]
		struct IamResponse {
			access_token: String,
			expires_in: u16
		}

		let clnt = blocking::Client::new();
		let url = Url::parse_with_params("https://iam.cloud.ibm.com/identity/token", &[
			("grant_type", "urn:ibm:params:oauth:grant-type:apikey"),
			("apikey", api_key)
			])?;
		let resp = clnt.post(url)
		.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
		.header(header::ACCEPT, "application/json").send()?;
		let resp: IamResponse =  resp.json()?;
		Ok((resp.access_token, resp.expires_in))
	}

	fn get(&mut self, api_key: &str) -> Result<&str, OnlineSttError> {
		let must_redo = if let Some(ref data) = self.data {
			Instant::now() > data.1
		}
		else {
			true
		};

		if must_redo {
			// A token is valid for 3600 seconds (60 minutes), but to be on the 
			// safe side let's give it 3480 seconds (58 minutes).
			let (token, valid_time) = Self::gen_iam_token(api_key)?;
			self.data = Some((token, Instant::now() + Duration::new((valid_time - (2 * 60)).into(),0)));
		}

		if let Some(ref data) = self.data  {
			Ok(&data.0)
		}
		else {
			panic!("TokenCache.data has no value, but we just set it");
		}
	}

}

impl IbmSttEngine {

	pub fn new(data: IbmSttData) -> Self {
		let location = hashmap! {
			"Dallas".to_owned() => "us-south",
			"Washington, DC".to_owned() => "us-east",
			"Frankfurt".to_owned() => "eu-de",
			"Sydney".to_owned() => "au-syd",
			"Tokyo".to_owned() => "jp-tok",
			"London".to_owned() => "eu-gb",
			"Seoul".to_owned() => "kr-seo"
		};
		IbmSttEngine{curr_socket: None, token_cache: TokenCache::new(&data.api_key), data: IbmSttData {
			api_key: data.api_key,
			instance: data.instance,
			gateway: location[&data.gateway].to_owned()
		}}
	}

	// Send all audio in one big chunk
	pub fn decode(&mut self, audio: &AudioRaw, model: &str) -> Result<Option<DecodeRes>, OnlineSttError> {
		let mut socket = WatsonSocket::new(model, self.data.clone(), self.token_cache.get(&self.data.api_key)?)?;
		socket.send_order(WatsonOrder::Start)?;
		socket.send_audio(audio)?;
		socket.send_order(WatsonOrder::Stop)?;
		let res = socket.get_answer();
		if let Err(err) =  socket.close() {
			warn!("Error while closing websocket: {:?}", err);
		}

		res
	}

	pub fn live_process_begin(&mut self, model: &str) -> Result<(), OnlineSttError> {
		let mut socket = WatsonSocket::new(model, self.data.clone(), self.token_cache.get(&self.data.api_key)?)?;
		socket.send_order(WatsonOrder::Start)?;
		self.curr_socket = Some(socket);
		
		Ok(())
	}

	pub fn live_process(&mut self, audio: &crate::audio::AudioRaw) -> Result<(), OnlineSttError> {
		let socket = {
			if let Some(ref mut sck) = self.curr_socket {
				sck
			}
			else {
				panic!("IbmSttEngine.live_process can't be called before live_proces_begin");
			}
		};

		socket.send_audio(audio)?;

	    Ok(())
	}
	pub fn live_process_end(&mut self) -> Result<Option<DecodeRes>, OnlineSttError> {
		let socket = {
			if let Some(ref mut sck) = self.curr_socket {
				sck
			}
			else {
				panic!("live_process_end can't be called twice")
			}
		};

		socket.send_order(WatsonOrder::Stop)?;
		let res = socket.get_answer();
		if let Some(sock) = std::mem::replace(&mut self.curr_socket, None) {
			if let Err(err) =  sock.close() {
				warn!("Error while closing websocket: {:?}", err);
			}
		}

		res
	}

}

