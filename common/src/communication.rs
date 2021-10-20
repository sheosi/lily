use crate::other::ConnectionConf;
use crate::vars::DEFAULT_HOTWORD_SENSITIVITY;

use anyhow::{anyhow, Result};
use rumqttc::{AsyncClient, EventLoop, LastWill, MqttOptions};
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Deserialize, Serialize)]
pub struct MsgAnswer {
    #[serde(skip_serializing_if = "Option::is_none", default = "no_vec")]
    pub audio: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none", default = "no_vec")]
    pub text: Option<Vec<u8>>
}

fn no_vec() -> Option<Vec<u8>> {None}

#[derive(Debug, Deserialize, Serialize)]
pub enum RequestData {
    Audio{data: Vec<u8>, is_final: bool},
    Text(String)
}
#[derive(Debug, Deserialize, Serialize)]
pub struct MsgRequest {
    #[serde(flatten)]
    pub data: RequestData,
    pub satellite: String,
}

// Message sent when new
#[derive(Deserialize, Serialize)]
pub struct MsgWelcome {
    pub conf: ClientConf,
    pub satellite: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MsgEvent {
    pub satellite: String,
    pub event: String
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ClientConf {
    pub hotword_sensitivity: f32
}

impl Default for ClientConf {
    fn default() -> Self {
        Self{
            hotword_sensitivity: DEFAULT_HOTWORD_SENSITIVITY
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct MsgNewSatellite {
    pub uuid: String,
    pub caps: Vec<String>
}


#[derive(Deserialize, Serialize)]
pub struct MsgGoodbye {
    pub satellite: String
}

#[derive(Serialize)]
pub struct ConnectionConfResolved {
    pub url_str: String,
    pub name: String,
    pub user_pass: Option<(String, String)>
}

impl ConnectionConfResolved {
    pub fn from<F: FnOnce()->String>(conf: ConnectionConf, make_uuid: F) -> Self {
        Self{
            url_str: conf.url_str,
            name: conf.name.unwrap_or_else(make_uuid),
            user_pass: conf.user_pass,
        }
    }
}

pub fn make_mqtt_conn(conf: &ConnectionConfResolved, last_will: Option<LastWill>) ->  Result<(AsyncClient, EventLoop)> {
    let url = Url::parse(
        &format!("http://{}",conf.url_str) // Let's add some protocol
    )?;
    let host = url.host_str().ok_or_else(||anyhow!("Coudln't get host from URL"))?;
    let port: u16 = url.port().unwrap_or(1883);
    
    // Init MQTT
    let mut mqttoptions = MqttOptions::new(&conf.name, host, port);
    if let Some(will) = last_will {
        mqttoptions.set_last_will(will);
    }
    mqttoptions.set_keep_alive(5);
    const MAX_PACKET_SIZE: usize = 100 * 1024 * 1024*8;
    mqttoptions.set_max_packet_size(MAX_PACKET_SIZE, MAX_PACKET_SIZE);
    match &conf.user_pass {
        Some((user, pass)) => {
            mqttoptions.set_credentials(user, pass);
        },
        None => {}
    }
    
    Ok(AsyncClient::new(mqttoptions, 10))
}
