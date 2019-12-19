mod stt;
mod tts;
mod audio;
mod gtts;
mod nlu;
mod vars;
mod hotword;
mod python;
mod packages;

// Standard library
use std::rc::Rc;
use core::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;

// This crate
use crate::audio::{RecDevice, PlayDevice, Recording};
use crate::python::{yaml_to_python, add_to_sys_path, python_init};
use crate::hotword::{HotwordDetector, Snowboy};
use crate::nlu::Nlu;
use crate::vars::*;
use crate::packages::load_packages;

// Other crates
use log::{info, warn};
use ref_thread_local::{RefThreadLocal, ref_thread_local};
use unic_langid::LanguageIdentifier;
use cpython::{Python, PyTuple, PyDict, ObjectProtocol, PyClone};

// To be shared on the same thread
ref_thread_local! {
    static managed TTS: Box<dyn crate::tts::Tts> = tts::TtsFactory::dummy();
}

enum ProgState {
    WaitingForHotword,
    Listening,
}

// Main loop, waits for hotword then records, acts and starts agian
fn record_loop() {
    // Set language
    let curr_lang : LanguageIdentifier = get_locale_default().parse().expect("Locale parsing failed");

    let mut order_map = load_packages(&Path::new(&resolve_path(PACKAGES_PATH)), &curr_lang);

    *TTS.borrow_mut() = tts::TtsFactory::load(&curr_lang, false);
    let snowboy_path = resolve_path(SNOWBOY_DATA_PATH);

    let mut record_device = RecDevice::new();
    let mut _play_device = PlayDevice::new();

    let mut stt = stt::SttFactory::load(&curr_lang, false);
    let mut hotword_detector = Snowboy::new(&snowboy_path.join("lily.pmdl"), &snowboy_path.join("common.res"));

    info!("Init Nlu");
    let nlu = Nlu::new(&resolve_path(NLU_ENGINE_PATH));
    let mut current_state = ProgState::WaitingForHotword;


    info!("Record start");

    // Start recording
    record_device.start_recording().unwrap();
    hotword_detector.start_hotword_check();

    order_map.call_order("lily_start");

    let mut current_speech = crate::audio::Audio{buffer: Vec::new(), samples_per_second: 16000};

    loop {
        let microphone_data = match record_device.read() {
            Some(d) => d,
            None => continue,
        };

        match current_state {
            ProgState::WaitingForHotword => {
                match hotword_detector.check_hotword(microphone_data) {
                    true => {
                        // Don't record for a moment
                        record_device.stop_recording().unwrap();
                        current_state = ProgState::Listening;
                        stt.begin_decoding().unwrap();
                        info!("Hotword detected");
                        order_map.call_order("init_reco");
                        record_device.start_recording().unwrap();
                    }
                    _ => {}
                }
            }
            ProgState::Listening => {
                current_speech.append_audio(microphone_data, 16000);

                match stt.decode(microphone_data).unwrap() {
                    stt::DecodeState::NotStarted => {},
                    stt::DecodeState::StartListening => {
                        info!("Listening speech");
                    }
                    stt::DecodeState::NotFinished => {}
                    stt::DecodeState::Finished(decode_res) => {
                        info!("End of speech");
                        current_state = ProgState::WaitingForHotword;
                        match decode_res {
                            None => warn!("Not recognized"),
                            Some((hypothesis, _utt_id, _score)) => {
                                if !hypothesis.is_empty() {
                                    record_device.stop_recording().unwrap();
                                    /*for seg in ps_decoder.seg_iter() {
                                        println!("{} : {}, {}",seg.word(), seg.prob().ascr, seg.prob().lscr);

                                    }*/
                                    let result = nlu.parse(&hypothesis).unwrap();
                                    let result_json = Nlu::to_json(&result);
                                    info!("{}", result_json);
                                    let score = result.intent.confidence_score;
                                    info!("Score: {}",score);

                                    // Do action if at least we are 80% confident on
                                    // what we got
                                    if score >= 0.55 {
                                        info!("Let's call an action");
                                        order_map.call_order(&result.intent.intent_name.unwrap());
                                        info!("Action called");
                                    }
                                    else {
                                        order_map.call_order("unrecognized");
                                    }
                                    record_device.start_recording().unwrap();
                                    hotword_detector.start_hotword_check();
                                    current_speech.write_wav(resolve_path(LAST_SPEECH_PATH).to_str().unwrap());
                                    current_speech.clear();
                                }
                                else {
                                    order_map.call_order("empty_reco");
                                }
                            }   
                        }
                    }
                }
            }
        }
    }
}

fn init_log() {
    let formatter = syslog::Formatter3164 {
        facility: syslog::Facility::LOG_USER,
        hostname: None,
        process: "lily".into(),
        pid: 0,
    };



    let logger = syslog::unix(formatter).expect("could not connect to syslog");
    log::set_boxed_logger(Box::new(syslog::BasicLogger::new(logger)))
            .map(|()| log::set_max_level(log::LevelFilter::Info)).ok();
            //simple_logger::init().unwrap();

}

pub struct ActionRegistry {
    map: HashMap<String, cpython::PyObject>
}
impl Clone for ActionRegistry {
    fn clone(&self) -> Self {
        // Get GIL
        let gil = Python::acquire_gil();
        let python = gil.python();

        let dup_refs = |pair:(&String, &cpython::PyObject)| {
            let (key, val) = pair;
            (key.to_owned(), val.clone_ref(python))
        };

        let new_map: HashMap<String, cpython::PyObject> = self.map.iter().map(dup_refs).collect();
        Self{map: new_map}
    }
}

impl ActionRegistry {

    fn new(actions_path: &Path, curr_lang: &LanguageIdentifier) -> Self {
        let mut reg = Self{map: HashMap::new()};
        reg.add_folder(actions_path, curr_lang);
        reg
    }

    fn add_folder(&mut self, actions_path: &Path, curr_lang: &LanguageIdentifier) {
        // Get GIL
        let gil = Python::acquire_gil();
        let python = gil.python();

        // Add folder to sys.path
        add_to_sys_path(python, actions_path).unwrap();
        info!("Add folder: {}", actions_path.to_str().unwrap());

        // Make order_map from python's modules
        for entry in std::fs::read_dir(actions_path).unwrap() {
            let entry = entry.unwrap();
            if entry.path().is_dir() {
                let mod_name = entry.file_name().into_string().unwrap().to_string();
                python.import(&mod_name).unwrap();
            }
        }

        let lily_py_mod = python.import("lily_ext").unwrap();

        // The path is the Python path, so set the current directory to it's parent (the package)
        let canon_path = actions_path.parent().unwrap().canonicalize().unwrap();
        info!("Actions_path:{}", canon_path.to_str().unwrap());
        std::env::set_current_dir(canon_path).unwrap();
        lily_py_mod.call(python, "__set_translations", (curr_lang.to_string(),), None).unwrap();
        
        for (key, val) in lily_py_mod.get(python, "action_classes").unwrap().cast_into::<PyDict>(python).unwrap().items(python) {
            self.map.insert(key.to_string(), val.clone_ref(python));
            println!("{:?}:{:?}", key.to_string(), val.to_string());
        }
    }

    fn clone_adding(&self, new_actions_path: &Path, curr_lang: &LanguageIdentifier) -> Self {
        let mut new = self.clone();
        new.add_folder(new_actions_path, curr_lang);
        new
    }

    fn clone_try_adding(&self, new_actions_path: &Path, curr_lang: &LanguageIdentifier) -> Self {
        if new_actions_path.is_dir() {
            self.clone_adding(new_actions_path, curr_lang)
        }
        else {
            self.clone()
        }
    }

    fn get(&self, action_name: &str) -> Option<&cpython::PyObject> {
        self.map.get(action_name)
    }
}

struct ActionData {
    obj: cpython::PyObject,
    args: cpython::PyObject
}

struct ActionSet {
    acts: Vec<ActionData>
}

impl ActionSet {
    fn create() -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {acts: Vec::new()}))
    }
    fn add_action(&mut self, py: Python, act_name: &str, yaml: &yaml_rust::Yaml, action_registry: &ActionRegistry) {
        self.acts.push(ActionData{obj: action_registry.get(act_name).unwrap().clone_ref(py), args: yaml_to_python(&yaml, py)});
    }
    fn call_all(&mut self, py: Python) {
        for action in self.acts.iter() {
            let trig_act = action.obj.getattr(py, "trigger_action").unwrap();
            trig_act.call(py, PyTuple::new(py, &[action.args.clone_ref(py)]), None).unwrap();
        }
    }
}

pub struct OrderMap {
    map: HashMap<String, Rc<RefCell<ActionSet>>>
}

impl OrderMap {
    fn new() -> Self {
        Self{map: HashMap::new()}
    }

    fn add_order(&mut self, order_name: &str, act_set: Rc<RefCell<ActionSet>>) {
        let action_entry = self.map.entry(order_name.to_string()).or_insert(ActionSet::create());
        *action_entry = act_set;
    }

    fn call_order(&mut self, act_name: &str) {
        if let Some(action_set) = self.map.get_mut(act_name) {
            let gil = Python::acquire_gil();
            let python = gil.python();

            action_set.borrow_mut().call_all(python);
        }
    }
}

fn get_locale_default() -> String {
    for (tag, val) in locale_config::Locale::user_default().tags() {
        if let None = tag {
            return format!("{}", val)
        }
    }

    "".to_string()
}

fn main() {
    init_log();
    python_init();
    record_loop();
}