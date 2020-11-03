use crate::vars::DEFAULT_HOTWORD_SENSITIVITY;
use serde::{Deserialize, Serialize};
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
