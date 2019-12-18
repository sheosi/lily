use std::path::PathBuf;
use ref_thread_local::{RefThreadLocal, ref_thread_local};

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
