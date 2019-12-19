use std::path::PathBuf;
use ref_thread_local::{RefThreadLocal, ref_thread_local};

// Paths
pub const NLU_ENGINE_PATH: &str = "resources/nlu/engine";
pub const NLU_TRAIN_SET_PATH: &str = "resources/nlu/train-set.json";
pub const STT_DATA_PATH: &str = "resources/stt";
pub const PICO_DATA_PATH: &str = "resources/tts";
pub const SNOWBOY_DATA_PATH: &str = "resources/hotword";
pub const PYTHON_SDK_PATH: &str = "resources/python";
pub const PACKAGES_PATH: &str = "packages";	
pub const LAST_SPEECH_PATH: &str = "last_speech.wav";


ref_thread_local! {
    static managed ORG_PATH: PathBuf = std::env::current_dir().unwrap().canonicalize().unwrap();
}

pub fn resolve_path(path: &str) -> PathBuf {
	ORG_PATH.borrow().join(path)
}

// Messages
pub const WRONG_YAML_KEY_MSG: &str = "A Yaml entry must be string convertable, report this together with the Yaml that caused this error";
pub const WRONG_YAML_ROOT_MSG: &str = "A 'skill_defs.yaml' file must start with a hash";
pub const WRONG_YAML_SECTION_TYPE_MSG: &str = "A skill section must be a hash";
pub const PACKAGES_PATH_ERR_MSG: &str = "Packages folder can't be read";