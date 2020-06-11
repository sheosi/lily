use std::path::PathBuf;
use lazy_static::lazy_static;

// Paths
pub const NLU_ENGINE_PATH: PathRef = PathRef::new("resources/nlu/engine");
pub const NLU_TRAIN_SET_PATH: PathRef = PathRef::new("resources/nlu/train-set.json");
pub const STT_DATA_PATH: PathRef = PathRef::new("resources/stt");
pub const DEEPSPEECH_DATA_PATH: PathRef = PathRef::new("resources/stt/deepspeech");
pub const PICO_DATA_PATH: PathRef = PathRef::new("resources/tts");
pub const SNOWBOY_DATA_PATH: PathRef = PathRef::new("resources/hotword");
pub const PYTHON_SDK_PATH: PathRef = PathRef::new("resources/python");
pub const PACKAGES_PATH: PathRef = PathRef::new("packages");
pub const LAST_SPEECH_PATH: PathRef = PathRef::new("last_speech.ogg");
pub const MAIN_CONF_PATH: PathRef = PathRef::new("conf.yaml");

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
pub const AUDIO_REC_START_ERR_MSG: &str = "Failed while trying to start audio recording, please report this";
pub const AUDIO_REC_STOP_ERR_MSG: &str = "Failed while trying to stop audio recording, please report this";
pub const CLOCK_TOO_EARLY_MSG :&str = "Somehow the system's clock time is before unix epoch, this is not supported, check your system's time and the CMOS battery";
pub const PYDICT_SET_ERR_MSG :&str = "Failed while assigning an entry in PyDict";
pub const NO_KEY_MSG: &str = "This shouldn't happen, a key checked beforehand is None";
pub const NO_YAML_FLOAT_MSG: &str = "This shouldn't happen, a Yaml was checked as a number which is not an u64 was not an f64 either";
pub const NO_COMPATIBLE_LANG_MSG: &str = "Lang negotiation failed, even though a default lang was provided";
// Other
pub const LILY_VER: &str = "0.1";
pub const DEFAULT_HOTWORD_SENSITIVITY: f32 = 0.45;
pub const MIN_SCORE_FOR_ACTION: f32 = 0.3;
pub const DEFAULT_SAMPLES_PER_SECOND: u32 = 16000;
pub const HOTWORD_CHECK_INTERVAL_MS: u16 = 100; // Larger = less CPU, more wait time