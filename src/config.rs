use serde::Deserialize;
use anyhow::{anyhow, Result};
use crate::vars::{DEFAULT_HOTWORD_SENSITIVITY, MAIN_CONF_PATH, resolve_path};

#[derive(Deserialize, Debug)]
pub struct Config {
    #[serde(default = "false_val")]
    pub prefer_online_tts: bool,
    #[serde(default = "false_val")]
    pub prefer_online_stt: bool,
    #[serde(default = "none_str")]
    pub ibm_tts_key: Option<String>,
    #[serde(default = "none_str")]
    pub ibm_stt_key: Option<String>,
    #[serde(default = "none_str")]
    pub ibm_gateway: Option<String>,
    #[serde(default = "def_hotword_sensitivity")]
    pub hotword_sensitivity: f32
}

fn false_val() -> bool {
    false
}

fn none_str() -> Option<String> {
    None
}

fn def_hotword_sensitivity() -> f32 {
    DEFAULT_HOTWORD_SENSITIVITY
}

pub fn get_conf() -> Config {
    load_conf().unwrap_or(Config{prefer_online_tts: false, prefer_online_stt: false, ibm_tts_key: None, ibm_stt_key: None, ibm_gateway: None, hotword_sensitivity: DEFAULT_HOTWORD_SENSITIVITY})
}

fn load_conf() -> Result<Config> {
    let conf_path = resolve_path(MAIN_CONF_PATH);
    if conf_path.is_file() {
        let conf_file = std::fs::File::open(conf_path)?;
        Ok(serde_yaml::from_reader(std::io::BufReader::new(conf_file))?)
    }
    else {
        Err(anyhow!("Config file not found"))
    }

}

impl Config {
    pub fn extract_ibm_tts_data(&self) -> Option<(String, String)> {
        if self.ibm_gateway.is_some() && self.ibm_tts_key.is_some() {
            Some((self.ibm_gateway.clone().unwrap(), self.ibm_tts_key.clone().unwrap()))
        }
        else {
            None
        }
    }

    pub fn extract_ibm_stt_data(&self) -> Option<(String, String)> {
        if self.ibm_gateway.is_some() && self.ibm_stt_key.is_some() {
            // Those unwrap cannot fail
            Some((self.ibm_gateway.clone().unwrap(), self.ibm_stt_key.clone().unwrap()))
        }
        else {
            None
        }
    }
}