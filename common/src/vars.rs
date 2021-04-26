use std::path::PathBuf;
use lazy_static::lazy_static;

#[cfg(not(debug_assertions))]
use std::path::Path;
#[cfg(not(debug_assertions))]
use dirs::data_local_dir;

use std::sync::Mutex;

// Paths
#[cfg(debug_assertions)]
lazy_static! {
    static ref ORG_PATH: PathBuf = std::env::current_dir().expect("Couldn't get current_dir").canonicalize().expect("Failed to canonicalize current_dir");
    static ref ASSETS_PATH: Mutex<PathBuf> = Mutex::new(ORG_PATH.join("resources"));
    static ref USER_DATA_PATH: PathBuf = ORG_PATH.join("debug_run");
}

#[cfg(not(debug_assertions))]
lazy_static! {
    static ref ASSETS_PATH: Mutex<PathBuf> = Mutex::new(PathBuf::new());
    static ref USER_DATA_PATH: PathBuf = data_local_dir().expect("No home dir");
}

#[cfg(debug_assertions)]
pub fn set_app_name(_name: &str){}

#[cfg(not(debug_assertions))]
pub fn set_app_name(name: &str) {
    (*(*ASSETS_PATH).lock().unwrap()) = Path::new("/usr/share").join(name).to_path_buf();
}

enum PathRefKind {
    UserData, Own
}
pub struct PathRef {
    path_ref: &'static str,
    kind: PathRefKind,
}

impl PathRef {
    pub const fn own(path_ref: &'static str) -> Self {
        Self{path_ref, kind: PathRefKind::Own}
    }

    pub const fn user_cfg(path_ref: &'static str) -> Self {
        Self{path_ref, kind: PathRefKind::UserData}
    }

    pub fn resolve(&self) -> PathBuf {
        match self.kind {
            PathRefKind::UserData => USER_DATA_PATH.join(self.path_ref),
            PathRefKind::Own => (*ASSETS_PATH.lock().unwrap()).join(self.path_ref)
        }
    }
}

pub struct MultipathRef {
    refs: &'static [PathRef]
}

impl MultipathRef {
    pub const fn new(refs: &'static [PathRef]) -> Self {
        Self {refs}
    }

    // Will get the first that exists from the list
    pub fn get(&self) -> PathBuf {
        for r in self.refs {
            let p = r.resolve();
            if p.exists() {
                return p;
            }
        }

        self.refs[0].resolve()
    }

    pub fn save_path(&self) -> PathBuf {
        self.refs[0].resolve()
    }

    pub fn all(&self) -> Vec<PathBuf> {
        self.refs.iter()
        .map(|r|r.resolve())
        .filter(|p|p.exists())
        .collect()
    }
}

// Messages
pub const AUDIO_REC_START_ERR_MSG: &str = "Failed while trying to start audio recording, please report this";
pub const AUDIO_REC_STOP_ERR_MSG: &str = "Failed while trying to stop audio recording, please report this";
pub const CLOCK_TOO_EARLY_MSG :&str = "Somehow the system's clock time is before unix epoch, this is not supported, check your system's time and the CMOS battery";

// Other
pub const LILY_VER: &str = std::env!("CARGO_PKG_VERSION");
pub const DEFAULT_HOTWORD_SENSITIVITY: f32 = 0.43;
pub const DEFAULT_SAMPLES_PER_SECOND: u32 = 16000;
pub const HOTWORD_CHECK_INTERVAL_MS: u16 = 20; // Larger = less CPU, more wait time
pub const ACTIVE_LISTENING_INTERVAL_MS: u16 = 200; // Larger = less CPU, more wait time
pub const RECORD_BUFFER_SIZE: usize = 32_000; // This ammounts to 2s of audio
pub const MAX_SAMPLES_PER_SECOND: u32 = 48_000;