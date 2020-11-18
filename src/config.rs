use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::stt::IbmSttData;
use crate::tts::{IbmTtsData, TtsData};
use crate::vars::{DEFAULT_HOTWORD_SENSITIVITY, MAIN_CONF_PATH};

use anyhow::{anyhow, Result};
use lily_common::communication::ClientConf;
use lily_common::other::ConnectionConf;
use serde::Deserialize;
use serde_yaml::Value;

thread_local! {
     pub static GLOBAL_CONF: RefCell<Rc<Config>> = RefCell::new(Rc::new(Config::default()));
}

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default = "false_val")]
    pub prefer_online_tts: bool,
    #[serde(default = "false_val")]
    pub prefer_online_stt: bool,
    #[serde(default = "none::<IbmSttData>")]
    pub ibm_stt: Option<IbmSttData>,
    #[serde(default = "none::<IbmTtsData>")]
    pub ibm_tts: Option<IbmTtsData>,
    #[serde(default = "none::<String>")]
    pub language: Option<String>,
    #[serde(default = "def_hotword_sensitivity")]
    pub hotword_sensitivity: f32,
    #[serde(default = "false_val")]
    pub debug_record_active_speech: bool,
    #[serde(default)]
    pub tts: TtsData,

    #[serde(default)]
    pub mqtt: ConnectionConf,


    #[serde(flatten)]
    pub pkgs_conf: HashMap<String, Value>
}

fn false_val() -> bool {
    false
}

fn none<T>()-> Option<T> {
    None
}

fn def_hotword_sensitivity() -> f32 {
    DEFAULT_HOTWORD_SENSITIVITY
}

impl Default for Config {
    fn default() -> Self {
        Config{
            prefer_online_tts: false,
            prefer_online_stt: false,
            ibm_stt: None,
            ibm_tts: None,
            language: None,
            hotword_sensitivity: DEFAULT_HOTWORD_SENSITIVITY,
            debug_record_active_speech: false,
            pkgs_conf: HashMap::new(),
            mqtt: ConnectionConf::default(),
            tts: TtsData::default()
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let conf_path = MAIN_CONF_PATH.resolve();
        if conf_path.is_file() {
            let conf_file = std::fs::File::open(conf_path)?;
            Ok(serde_yaml::from_reader(std::io::BufReader::new(conf_file))?)
        }
        else {
            Err(anyhow!("Config file not found"))
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

    pub fn to_client_conf(&self) -> ClientConf {
        ClientConf {
            hotword_sensitivity: self.hotword_sensitivity
        }
    }
}
