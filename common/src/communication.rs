use crate::other::ConnectionConf;
use crate::vars::DEFAULT_HOTWORD_SENSITIVITY;

use rumqttc::{AsyncClient, EventLoop, MqttOptions};
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Deserialize, Serialize)]
pub struct MsgAnswerVoice {
    pub data: Vec<u8>
}

#[derive(Deserialize, Serialize)]
pub struct MsgNluVoice {
    pub audio: Vec<u8>,
    pub is_final: bool,
    pub satellite: String,
}

// Message sent when new
#[derive(Deserialize, Serialize)]
pub struct MsgWelcome {
    pub conf: ClientConf,
    pub satellite: String,
}

#[derive(Deserialize, Serialize)]
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
    pub uuid: String
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

pub fn make_mqtt_conn(conf: &ConnectionConfResolved) ->  (AsyncClient, EventLoop) {
    let url = Url::parse(
        &format!("http://{}",conf.url_str) // Let's add some protocol
    ).unwrap();
    let host = url.host_str().unwrap();
    let port: u16 = url.port().unwrap_or(1883);
    
    // Init MQTT
    let mut mqttoptions = MqttOptions::new(&conf.name, host, port);
    mqttoptions.set_keep_alive(5);
    match &conf.user_pass {
        Some((user, pass)) => {
            mqttoptions.set_credentials(user, pass);
        },
        None => {}
    }
    
    AsyncClient::new(mqttoptions, 10)
}
