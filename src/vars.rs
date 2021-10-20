use lily_common::vars::{MultipathRef, PathRef};

// Paths
pub const STT_DATA_PATH: PathRef = PathRef::own("stt");
#[cfg(feature = "deepspeech_stt")]
pub const DEEPSPEECH_DATA_PATH: PathRef = PathRef::own("stt/deepspeech");
pub const PICO_DATA_PATH: PathRef = PathRef::own("tts");
#[cfg(feature = "python_skills")]
pub const PYTHON_SDK_PATH: PathRef = PathRef::own("python");

#[cfg(not(feature = "devel_rasa_nlu"))]
pub const NLU_ENGINE_PATH: PathRef = PathRef::user_cfg("data/nlu/engine");
#[cfg(not(feature = "devel_rasa_nlu"))]
pub const NLU_TRAIN_SET_PATH: PathRef = PathRef::user_cfg("data/nlu/train-set.json");
#[cfg(feature = "devel_rasa_nlu")]
pub const NLU_RASA_PATH: PathRef = PathRef::user_cfg("data/nlu/rasa");

#[cfg(debug_assertions)]
pub const PS_LOG_PATH: PathRef = PathRef::user_cfg("logs/pocketsphinx.log");

pub const MAIN_CONF_PATH: PathRef = PathRef::user_cfg("conf.yaml");
pub const PYTHON_VIRTUALENV: PathRef = PathRef::user_cfg("data/python_env");
pub const SKILLS_PATH: MultipathRef = MultipathRef::new(&[
    PathRef::user_cfg("skills"),
    PathRef::own("skills")
]);


// Messages
#[cfg(feature = "python_skills")]
pub const SKILLS_PATH_ERR_MSG: &str = "Skills folder can't be read";
#[cfg(feature = "python_skills")]
pub const PYDICT_SET_ERR_MSG :&str = "Failed while assigning an entry in PyDict";
#[cfg(feature = "python_skills")]
pub const NO_YAML_FLOAT_MSG: &str = "This shouldn't happen, a Yaml Value was checked as a number which is not an u64 was not an f64 either";
pub const NO_COMPATIBLE_LANG_MSG: &str = "Lang negotiation failed, even though a default lang was provided";
#[cfg(feature = "deepspeech_stt")]
pub const ALPHA_BETA_MSG: &str = "Setting alpha and beta failed, though this shouldn't happen";
#[cfg(feature = "deepspeech_stt")]
pub const SET_BEAM_MSG: &str = "Setting beam's width this wasn't expected to happen";
#[cfg(feature = "deepspeech_stt")]
pub const DEEPSPEECH_READ_FAIL_MSG: &str = "Failed to read deepspeech's folder";
pub const UNEXPECTED_MSG: &str = "Something unexpected (and probably terrible) happened, this should be reported";
pub const POISON_MSG: &str = "A shared lock had a panic in another thread";
pub const NO_ADD_ENTITY_VALUE_MSG: &str = "Can't add value to entity, NLU manager not yet ready";


// Other
pub const MIN_SCORE_FOR_ACTION: f32 = 0.3;
pub const NLU_TRAINING_DELAY: u64 = 1000;

pub fn mangle(skill_name: &str, intent_name: &str) -> String {
    format!("__{}__{}", skill_name, intent_name)
}
