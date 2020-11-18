use std::path::PathBuf;
use lazy_static::lazy_static;

// Paths
#[cfg(not(feature = "devel_rasa_nlu"))]
pub const NLU_ENGINE_PATH: PathRef = PathRef::new("resources/nlu/engine");
#[cfg(not(feature = "devel_rasa_nlu"))]
pub const NLU_TRAIN_SET_PATH: PathRef = PathRef::new("resources/nlu/train-set.json");
#[cfg(feature = "devel_rasa_nlu")]
pub const NLU_RASA_PATH: PathRef = PathRef::new("resources/nlu/rasa/");
pub const STT_DATA_PATH: PathRef = PathRef::new("resources/stt");
#[cfg(feature = "deepspeech_stt")]
pub const DEEPSPEECH_DATA_PATH: PathRef = PathRef::new("resources/stt/deepspeech");
pub const PICO_DATA_PATH: PathRef = PathRef::new("resources/tts");
pub const PYTHON_SDK_PATH: PathRef = PathRef::new("resources/python");
pub const MAIN_CONF_PATH: PathRef = PathRef::new("conf.yaml");
pub const PS_LOG_PATH: PathRef = PathRef::new("resources/stt/pocketsphinx.log");

lazy_static! {
    static ref ORG_PATH: PathBuf = std::env::current_dir().expect("Couldn't get current_dir").canonicalize().expect("Failed to canonicalize current_dir");
}

pub struct PathRef {
    path_ref: &'static str
}

impl PathRef {
    const fn new(path_ref: &'static str) -> Self {
        Self{path_ref}
    }

    pub fn resolve(&self) -> PathBuf {
        ORG_PATH.join(self.path_ref)
    }
}

// Messages
pub const WRONG_YAML_KEY_MSG: &str = "A Yaml entry must be string convertable, report this together with the Yaml that caused this error";
pub const WRONG_YAML_ROOT_MSG: &str = "A 'skill_defs.yaml' file must start with a hash";
pub const WRONG_YAML_SECTION_TYPE_MSG: &str = "A skill section must be a hash";
pub const PACKAGES_PATH_ERR_MSG: &str = "Packages folder can't be read";
pub const PYDICT_SET_ERR_MSG :&str = "Failed while assigning an entry in PyDict";
pub const NO_YAML_FLOAT_MSG: &str = "This shouldn't happen, a Yaml was checked as a number which is not an u64 was not an f64 either";
pub const NO_COMPATIBLE_LANG_MSG: &str = "Lang negotiation failed, even though a default lang was provided";
#[cfg(feature = "deepspeech_stt")]
pub const ALPHA_BETA_MSG: &str = "Setting alpha and beta failed, though this shouldn't happen";
#[cfg(feature = "deepspeech_stt")]
pub const SET_BEAM_MSG: &str = "Setting beam's width this wasn't expected to happen";
#[cfg(feature = "deepspeech_stt")]
pub const DEEPSPEECH_READ_FAIL_MSG: &str = "Failed to read deepspeech's folder";
// Other
pub const DEFAULT_HOTWORD_SENSITIVITY: f32 = 0.43;
pub const MIN_SCORE_FOR_ACTION: f32 = 0.3;
pub const DEFAULT_SAMPLES_PER_SECOND: u32 = 16000;
