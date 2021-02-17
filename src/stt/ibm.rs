use std::time::{Duration, Instant};

use crate::stt::{DecodeRes, OnlineSttError, Stt, SttInfo, SttConstructionError, SttBatched,SttError};

use async_trait::async_trait;
use fluent_langneg::{negotiate_languages, NegotiationStrategy};
use futures::{SinkExt, StreamExt};
use lily_common::audio::AudioRaw;
use lily_common::vars::DEFAULT_SAMPLES_PER_SECOND;
use maplit::hashmap;
use reqwest::{Client, header};
use tokio_tungstenite::{connect_async, WebSocketStream};
use tokio::net::TcpStream;
use tungstenite::Message;
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
	key: String,
	instance: String,
	gateway: String
}

impl IbmStt {

    pub async fn new(lang: &LanguageIdentifier, data: IbmSttData) -> Result<Self, SttConstructionError> {
        Ok(IbmStt{engine: IbmSttEngine::new(data).await, model: Self::model_from_lang(lang)?.to_string()})
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

#[async_trait(?Send)]
impl SttBatched for IbmStt {
    
    async fn decode(&mut self, audio: &[i16]) -> Result<Option<DecodeRes>, SttError> {
        Ok(self.engine.decode(&AudioRaw::new_raw(audio.to_vec(), DEFAULT_SAMPLES_PER_SECOND), &self.model).await?)
    }

    fn get_info(&self) -> SttInfo {
        SttInfo {name: "Ibm's Speech To Text".to_string(), is_online: true}
    }
}

#[async_trait(?Send)]
impl Stt for IbmStt {
	async fn begin_decoding(&mut self) -> Result<(), SttError> {
		self.engine.live_process_begin(&self.model).await?;
		Ok(())
	}
    async fn process(&mut self, audio: &[i16]) -> Result<(), SttError> {
        self.engine.live_process(&AudioRaw::new_raw(audio.to_vec(), 16000)).await?;
        Ok(())
    }
    async fn end_decoding(&mut self) -> Result<Option<DecodeRes>, SttError> {
        let res = self.engine.live_process_end().await?;
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
	confidence: f32,
	transcript: String
}


pub struct IbmSttEngine {
	curr_socket: Option <WatsonSocket>,
	data: IbmSttData,
	token_cache: TokenCache
}

struct WatsonSocket {
	socket: WebSocketStream<TcpStream>
}

enum WatsonOrder {
	Start,
	Stop
}

impl WatsonSocket {

	async fn new(model: &str, data: IbmSttData, token: &str) -> Result<Self, OnlineSttError> {
		let url_str = format!("wss://api.{}.speech-to-text.watson.cloud.ibm.com/instances/{}/v1/recognize", data.gateway, data.instance);
		let (socket, _response) =
			connect_async(Url::parse_with_params(&url_str,&[
				("access_token", token),
				("model", model)
			])?).await?;
		
		Ok(Self{socket})
	}

	async fn send_order(&mut self, order: WatsonOrder) -> Result<(), OnlineSttError> {
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
		self.socket.send(Message::Text(order_str)).await?;

		Ok(())
	}

	async fn send_audio(&mut self, audio: &AudioRaw) -> Result<(), OnlineSttError> {
		let as_ogg = audio.to_ogg_opus()?;
		self.socket
		.send(Message::Binary(as_ogg)).await?;

		Ok(())
	}

	async fn get_answer(&mut self) -> Result<Option<DecodeRes>, OnlineSttError> {
		loop {
			if let Message::Text(response_str) = 
				self.socket.next().await
				.ok_or(OnlineSttError::ConnectionClosed)?? {

				let response_res: Result<WatsonResponse,_> = serde_json::from_str(&response_str);
				if let Ok(response) = response_res {
					let res = {
						if !response.results.is_empty() {
							let alternatives = &response.results[response.result_index as usize].alternatives;

							if !alternatives.is_empty() {
								let res_str = &alternatives[0].transcript;
								Some(DecodeRes{
									hypothesis: res_str.to_string(),
									confidence: alternatives[0].confidence
								})
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

	async fn close(mut self) -> Result<(), OnlineSttError> {
		
		self.socket.close(None).await?;
		// TODO: Make sure we get Error::ConnectionClosed

		Ok(())
	}
}

struct TokenCache {
	data: Option<(String, Instant)>,
}

impl TokenCache {
	async fn new(key: &str) -> Self {
		let mut res = TokenCache{data: None};
		if let Err(err) = res.get(key).await {
			warn!("Initial IBM API key couldn't be obtained, continuing regardless: {:?}", err);
		}

		res
	}

	async fn gen_iam_token(key: &str) -> Result<(String, u16), OnlineSttError> {
		#[derive(Debug, Deserialize)]
		struct IamResponse {
			access_token: String,
			expires_in: u16
		}

		let clnt = Client::new();
		let url = Url::parse_with_params("https://iam.cloud.ibm.com/identity/token", &[
			("grant_type", "urn:ibm:params:oauth:grant-type:apikey"),
			("apikey", key)
			])?;
		let resp = clnt.post(url)
		.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
		.header(header::ACCEPT, "application/json").send().await?;
		let resp: IamResponse =  resp.json().await?;
		Ok((resp.access_token, resp.expires_in))
	}

	async fn get(&mut self, key: &str) -> Result<&str, OnlineSttError> {
		let must_redo = if let Some(ref data) = self.data {
			Instant::now() > data.1
		}
		else {
			true
		};

		if must_redo {
			// A token is valid for 3600 seconds (60 minutes), but to be on the 
			// safe side let's give it 3480 seconds (58 minutes).
			let (token, valid_time) = Self::gen_iam_token(key).await?;
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

	pub async fn new(data: IbmSttData) -> Self {
		let location = hashmap! {
			"Dallas".to_owned() => "us-south",
			"Washington, DC".to_owned() => "us-east",
			"Frankfurt".to_owned() => "eu-de",
			"Sydney".to_owned() => "au-syd",
			"Tokyo".to_owned() => "jp-tok",
			"London".to_owned() => "eu-gb",
			"Seoul".to_owned() => "kr-seo"
		};
		IbmSttEngine{curr_socket: None, token_cache: TokenCache::new(&data.key).await, data: IbmSttData {
			key: data.key,
			instance: data.instance,
			gateway: location[&data.gateway].to_owned()
		}}
	}

	// Send all audio in one big chunk
	pub async fn decode(&mut self, audio: &AudioRaw, model: &str) -> Result<Option<DecodeRes>, OnlineSttError> {
		let mut socket = WatsonSocket::new(model, self.data.clone(), self.token_cache.get(&self.data.key).await?).await?;
		socket.send_order(WatsonOrder::Start).await?;
		socket.send_audio(audio).await?;
		socket.send_order(WatsonOrder::Stop).await?;
		let res = socket.get_answer().await;
		if let Err(err) =  socket.close().await {
			warn!("Error while closing websocket: {:?}", err);
		}

		res
	}

	pub async fn live_process_begin(&mut self, model: &str) -> Result<(), OnlineSttError> {
		let mut socket = WatsonSocket::new(model, self.data.clone(), self.token_cache.get(&self.data.key).await?).await?;
		socket.send_order(WatsonOrder::Start).await?;
		self.curr_socket = Some(socket);
		
		Ok(())
	}

	pub async fn live_process(&mut self, audio: &AudioRaw) -> Result<(), OnlineSttError> {
		let socket = self.curr_socket.as_mut().expect("IbmSttEngine.live_process can't be called before live_proces_begin");
		socket.send_audio(audio).await?;

	    Ok(())
	}
	pub async fn live_process_end(&mut self) -> Result<Option<DecodeRes>, OnlineSttError> {
		let socket = self.curr_socket.as_mut().expect("live_process_end can't be called twice");

		socket.send_order(WatsonOrder::Stop).await?;
		let res = socket.get_answer().await;
		if let Some(sock) = std::mem::replace(&mut self.curr_socket, None) {
			if let Err(err) =  sock.close().await {
				warn!("Error while closing websocket: {:?}", err);
			}
		}

		res
	}

}

