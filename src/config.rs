// Standard library
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

// This crate
use crate::stt::SttData;
use crate::tts::TtsData;
use crate::vars::MAIN_CONF_PATH;

// Other crates
use anyhow::{anyhow, Result};
use lily_common::communication::ClientConf;
use lily_common::other::{false_val, none, ConnectionConf};
use lily_common::vars::DEFAULT_HOTWORD_SENSITIVITY;
use serde::Deserialize;
use serde_yaml::Value;

thread_local! {
    pub static GLOBAL_CONF: RefCell<Rc<Config>> = RefCell::new(Rc::new(Config::default()));
}

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default = "none::<Vec<String>>")]
    pub language: Option<Vec<String>>,
    #[serde(default = "def_hotword_sensitivity")]
    pub hotword_sensitivity: f32,
    #[serde(default = "false_val")]
    pub debug_record_active_speech: bool,
    #[serde(default)]
    pub tts: TtsData,

    #[serde(default)]
    pub stt: SttData,

    #[serde(default)]
    pub mqtt: ConnectionConf,

    #[serde(flatten)]
    pub skills_conf: HashMap<String, Value>,
}

fn def_hotword_sensitivity() -> f32 {
    DEFAULT_HOTWORD_SENSITIVITY
}

impl Default for Config {
    fn default() -> Self {
        Config {
            stt: SttData::default(),
            language: None,
            hotword_sensitivity: DEFAULT_HOTWORD_SENSITIVITY,
            debug_record_active_speech: false,
            skills_conf: HashMap::new(),
            mqtt: ConnectionConf::default(),
            tts: TtsData::default(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let conf_path = MAIN_CONF_PATH.resolve();
        if conf_path.is_file() {
            let conf_file = std::fs::File::open(conf_path)?;
            Ok(serde_yaml::from_reader(std::io::BufReader::new(conf_file))?)
        } else {
            Err(anyhow!("Config file not found"))
        }
    }

    pub fn to_client_conf(&self) -> ClientConf {
        ClientConf {
            hotword_sensitivity: self.hotword_sensitivity,
        }
    }
}
