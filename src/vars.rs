use lily_common::vars::PathRef;

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
pub const PYTHON_VIRTUALENV: PathRef = PathRef::new("resources/python/env");
pub const SKILLS_PATH: PathRef = PathRef::new("skills");


// Messages
pub const SKILLS_PATH_ERR_MSG: &str = "Skills folder can't be read";
pub const PYDICT_SET_ERR_MSG :&str = "Failed while assigning an entry in PyDict";
pub const NO_YAML_FLOAT_MSG: &str = "This shouldn't happen, a Yaml was checked as a number which is not an u64 was not an f64 either";
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


pub fn mangle(intent_name: &str, skill_name: &str) -> String {
    format!("__{}__{}", skill_name, intent_name)
}