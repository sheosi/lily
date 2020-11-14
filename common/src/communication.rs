use crate::other::ConnectionConf;
use crate::vars::DEFAULT_HOTWORD_SENSITIVITY;

use rumqttc::{AsyncClient, EventLoop, MqttOptions};
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

#[derive(Deserialize, Serialize)]
pub struct MsgAnswerVoice {
    pub data: Vec<u8>
}

#[derive(Deserialize, Serialize)]
pub struct MsgNluVoice {
    pub audio: Vec<u8>,
    pub is_final: bool
}

// Message sent when new
#[derive(Deserialize, Serialize)]
pub struct MsgWelcome {
    pub conf: ClientConf,
    pub uuid: Uuid,
    pub name: String
}

#[derive(Deserialize, Serialize)]
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
    pub name: String
}

pub fn make_mqtt_conn(conf: &ConnectionConf) ->  (AsyncClient, EventLoop) {
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
