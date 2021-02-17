use std::path::PathBuf;
use lazy_static::lazy_static;

// Paths
pub const SNOWBOY_DATA_PATH: PathRef = PathRef::new("resources/hotword");
pub const LAST_SPEECH_PATH: PathRef = PathRef::new("last_speech.ogg");
pub const MAIN_CONF_PATH: PathRef = PathRef::new("conf.yaml");

lazy_static! {
    static ref ORG_PATH: PathBuf = std::env::current_dir().expect("Couldn't get current_dir").canonicalize().expect("Failed to canonicalize current_dir");
}

pub struct PathRef {
    path_ref: &'static str
}

impl PathRef {
    pub const fn new(path_ref: &'static str) -> Self {
        Self{path_ref}
    }

    pub fn resolve(&self) -> PathBuf {
        ORG_PATH.join(self.path_ref)
    }
}

// Messages
pub const AUDIO_REC_START_ERR_MSG: &str = "Failed while trying to start audio recording, please report this";
pub const AUDIO_REC_STOP_ERR_MSG: &str = "Failed while trying to stop audio recording, please report this";
pub const CLOCK_TOO_EARLY_MSG :&str = "Somehow the system's clock time is before unix epoch, this is not supported, check your system's time and the CMOS battery";

// Other
pub const LILY_VER: &str = "0.6";
pub const DEFAULT_HOTWORD_SENSITIVITY: f32 = 0.43;
pub const DEFAULT_SAMPLES_PER_SECOND: u32 = 16000;
pub const HOTWORD_CHECK_INTERVAL_MS: u16 = 20; // Larger = less CPU, more wait time
pub const ACTIVE_LISTENING_INTERVAL_MS: u16 = 50; // Larger = less CPU, more wait time
pub const RECORD_BUFFER_SIZE: usize = 32_000; // This ammounts for 2s of audio
pub const MAX_SAMPLES_PER_SECOND: u32 = 48_000;