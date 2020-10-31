use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct MsgAnswerVoice {
    pub data: Vec<u8>
}

#[derive(Serialize)]
pub struct MsgNluVoice {
    pub audio: Vec<u8>,
    pub is_final: bool
}