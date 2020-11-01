use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use crate::interfaces::MqttConfig;
use crate::stt::IbmSttData;
use crate::vars::{DEFAULT_HOTWORD_SENSITIVITY, MAIN_CONF_PATH, NO_KEY_MSG};
use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_yaml::Value;

thread_local! {
     pub static GLOBAL_CONF: RefCell<Rc<Config>> = RefCell::new(Rc::new(Config::default()));
}

#[derive(Clone, Deserialize, Debug)]
pub struct Config {
    #[serde(default = "false_val")]
    pub prefer_online_tts: bool,
    #[serde(default = "false_val")]
    pub prefer_online_stt: bool,
    #[serde(default = "none_str")]
    pub ibm_tts_key: Option<String>,
    #[serde(default = "none_ibm_stt")]
    pub ibm_stt: Option<IbmSttData>,
    #[serde(default = "none_str")]
    pub ibm_gateway: Option<String>,
    #[serde(default = "none_str")]
    pub language: Option<String>,
    #[serde(default = "def_hotword_sensitivity")]
    pub hotword_sensitivity: f32,
    #[serde(default = "false_val")]
    pub debug_record_active_speech: bool,

    #[serde(default = "none_mqtt")]
    pub mqtt_conf: Option<MqttConfig>,


    #[serde(flatten)]
    pub pkgs_conf: HashMap<String, Value>
}

fn false_val() -> bool {
    false
}

fn none_ibm_stt() -> Option<IbmSttData> {
    None
}

fn none_mqtt() -> Option<MqttConfig> {
    None
}


fn none_str() -> Option<String> {
    None
}

fn def_hotword_sensitivity() -> f32 {
    DEFAULT_HOTWORD_SENSITIVITY
}

pub fn get_conf() -> Config {
    load_conf().unwrap_or(Config::default())
}

fn load_conf() -> Result<Config> {
    let conf_path = MAIN_CONF_PATH.resolve();
    if conf_path.is_file() {
        let conf_file = std::fs::File::open(conf_path)?;
        Ok(serde_yaml::from_reader(std::io::BufReader::new(conf_file))?)
    }
    else {
        Err(anyhow!("Config file not found"))
    }

}

impl Config {
    fn default() -> Self {
        Config{
            prefer_online_tts: false,
            prefer_online_stt: false,
            ibm_tts_key: None,
            ibm_stt: None,
            ibm_gateway: None,
            language: None,
            hotword_sensitivity: DEFAULT_HOTWORD_SENSITIVITY,
            debug_record_active_speech: false,
            pkgs_conf: HashMap::new(),
            mqtt_conf: None
        }
    }

    pub fn get_package_path(&self, pkg_name: &str, pkg_path: &str) -> Option<&str> {
        self.pkgs_conf.get(pkg_name).and_then(|m| {
            let mut curr_map = m;
            for path_part in pkg_path.split("/") {
                match curr_map.get(path_part) {
                    Some(inner_data) => curr_map = inner_data,
                    None => return None
                }
            }

            curr_map.as_str()
        })
    }

    pub fn extract_ibm_tts_data(&self) -> Option<(String, String)> {
        if self.ibm_gateway.is_some() && self.ibm_tts_key.is_some() {
            Some((self.ibm_gateway.clone().expect(NO_KEY_MSG), self.ibm_tts_key.clone().expect(NO_KEY_MSG)))
        }
        else {
            None
        }
    }

    pub fn extract_ibm_stt_data(&self) -> Option<IbmSttData> {
        if self.ibm_stt.is_some() {
            self.ibm_stt.clone()
        }
        else {
            None
        }
    }
}