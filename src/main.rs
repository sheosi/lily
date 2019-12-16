mod stt;
mod tts;
mod audio;
mod gtts;
mod nlu;
mod vars;
mod hotword;

use std::rc::Rc;
use core::cell::RefCell;
use cpython::{Python, PyList, PyTuple, PyDict, PyString, PythonObject, PyResult, ObjectProtocol, PyClone, py_module_initializer, py_fn, py_method_def};
use std::collections::HashMap;
use std::path::Path;

use crate::audio::{RecDevice, PlayDevice};
use crate::audio::Recording;
use crate::hotword::{HotwordDetector, Snowboy};
use ref_thread_local::{RefThreadLocal, ref_thread_local};

use log::{info, warn};
use yaml_rust::{YamlLoader, Yaml};
use crate::nlu::{Nlu, NluManager};
use crate::vars::{NLU_ENGINE_PATH, NLU_TRAIN_SET_PATH, SNOWBOY_DATA_PATH, PYTHON_SDK_PATH, PACKAGES_PATH};
use cpython::ToPyObject;
use unic_langid::LanguageIdentifier;

ref_thread_local! {
    static managed TTS: Box<dyn crate::tts::Tts> = tts::TtsFactory::dummy();
}

enum ProgState {
    WaitingForHotword,
    Listening,
}

fn record_loop() {
    // Set language
    let curr_lang : LanguageIdentifier = get_locale_default().parse().expect("Locale parsing failed");

    let mut order_map = load_packages(&Path::new(PACKAGES_PATH), &curr_lang);

    *TTS.borrow_mut() = tts::TtsFactory::load(&curr_lang, false);
    let snowboy_path = Path::new(SNOWBOY_DATA_PATH);

    // Init
    let mut record_device = RecDevice::new();
    let mut _play_device = PlayDevice::new();

    let mut stt = stt::SttFactory::load(&curr_lang, false);
    let mut hotword_detector = Snowboy::new(&snowboy_path.join("lily.pmdl"), &snowboy_path.join("common.res"));

    info!("Init Nlu");
    let nlu = Nlu::new(Path::new(NLU_ENGINE_PATH));
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
                                    if score >= 0.8 {
                                        info!("Let's call an action");
                                        order_map.call_order(&result.intent.intent_name.unwrap());
                                        info!("Action called");
                                    }
                                    else {
                                        order_map.call_order("unrecognized");
                                    }
                                    record_device.start_recording().unwrap();
                                    hotword_detector.start_hotword_check();
                                    current_speech.write_wav("last_speech.wav");
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

// Define executable module
py_module_initializer!(lily, initlily, PyInit_lily, |py, m| {
    m.add(py, "__doc__", "This module is implemented in Rust.")?;
    m.add(py, "say", py_fn!(py, python_say(input: &str)))?;

    Ok(())
});

fn python_say(python: Python, input: &str) -> PyResult<cpython::PyObject> {
    let audio = TTS.borrow_mut().synth_text(input).unwrap();
    PlayDevice::new().play(&*audio.buffer, audio.samples_per_second);
    Ok(python.None())
}

struct ActionRegistry {
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

    fn new(actions_path: &Path) -> Self {
        let mut reg = Self{map: HashMap::new()};
        reg.add_folder(actions_path);
        reg
    }

    fn add_folder(&mut self, actions_path: &Path) {
        // Get GIL
        let gil = Python::acquire_gil();
        let python = gil.python();

        // Add folder to sys.path
        let sys = python.import("sys").unwrap();
        let sys_path = sys.get(python, "path").unwrap().cast_into::<PyList>(python).unwrap();
        sys_path.insert_item(python, 1, PyString::new(python, actions_path.to_str().unwrap()).into_object());

        // Make order_map from python's modules
        for entry in std::fs::read_dir(actions_path).unwrap() {
            let entry = entry.unwrap();
            if entry.path().is_dir() {
                let mod_name = entry.file_name().into_string().unwrap().to_string();
                python.import(&mod_name).unwrap();
            }
        }

        let lily_py_mod = python.import("lily_sdk").unwrap();
        for (key, val) in lily_py_mod.get(python, "action_classes").unwrap().cast_into::<PyDict>(python).unwrap().items(python) {
            self.map.insert(key.to_string(), val.clone_ref(python));
            println!("{:?}:{:?}", key.to_string(), val.to_string());
        }
    }

    fn clone_adding(&self, new_actions_path: &Path) -> Self {
        let mut new = self.clone();
        new.add_folder(new_actions_path);
        new
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

struct OrderMap {
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

fn load_package(order_map: &mut OrderMap, nlu_man: &mut NluManager, action_registry: &ActionRegistry, path: &Path, _curr_lang: &LanguageIdentifier) {
    // Load Yaml
    let docs = YamlLoader::load_from_str(&std::fs::read_to_string(path.join("skills_def.yaml")).unwrap()).unwrap();

    // Multi document support, doc is a yaml::YamlLoader
    let doc = &docs[0];

    //Debug support
    println!("{:?}", docs);

    // Load actions + singals from Yaml
    for (key, data) in doc.as_hash().unwrap().iter() {
        if let Some(skill_def) = data.as_hash() {
            let skill_name = key.as_str().unwrap();
            println!("{}", skill_name);

            let mut actions: Vec<(&str, &yaml_rust::Yaml)> = Vec::new();
            let mut signals: Vec<(&str, &yaml_rust::Yaml)> = Vec::new();
            for (key2, data2) in skill_def.iter() {
                let as_str = key2.as_str().unwrap();
                
                match as_str {
                    "actions" => {
                        for (key3, data3) in data2.as_hash().unwrap().iter() {
                            actions.push((key3.as_str().unwrap(), data3));
                        }
                    }
                    "signals" => {
                        for (key3, data3) in data2.as_hash().unwrap().iter() {
                            signals.push((key3.as_str().unwrap(), data3));
                        }
                    }
                    _ => {}
                }

            }

            let act_set = ActionSet::create();
            for (act_name, act_arg) in actions.iter() {
                let gil = Python::acquire_gil();
                let py = gil.python();
                act_set.borrow_mut().add_action(py, act_name, act_arg, &action_registry);
            }
            for (sig_name, sig_arg) in signals.iter() {
                    

                if sig_name == &"order" {
                    if let Some(order_str) = sig_arg.as_str() {
                        nlu_man.add_intent(skill_name, vec![order_str.to_string()]);
                    }
                }
                else {
                    warn!("Unknown signal {} present in conf file", sig_name);
                }
            }

            order_map.add_order(skill_name, act_set);
        }
    }
}

fn load_packages(path: &Path, curr_lang: &LanguageIdentifier) -> OrderMap {
    let mut order_map = OrderMap::new();
    let mut nlu_man = NluManager::new();

    // TODO: Add into account own actions from the package
    let action_registry = ActionRegistry::new(Path::new(PYTHON_SDK_PATH));

    for entry in std::fs::read_dir(path).unwrap() {
        let entry = entry.unwrap().path();
        if entry.is_dir() {
            load_package(&mut order_map, &mut nlu_man, &action_registry.clone_adding(&entry.join("python")), &entry, curr_lang);
        }
    }

    nlu_man.train(Path::new(NLU_TRAIN_SET_PATH), Path::new(NLU_ENGINE_PATH), curr_lang);

    order_map
}

fn yaml_to_python(yaml: &yaml_rust::Yaml, py: Python) -> cpython::PyObject {
    match yaml {
        Yaml::Real(string) => {
            string.parse::<f64>().unwrap().into_py_object(py).into_object()
        }
        Yaml::Integer(int) => {
            int.into_py_object(py).into_object()

        }
        Yaml::Boolean(boolean) => {
            if *boolean {
                cpython::Python::True(py).into_object()
            }
            else {
                cpython::Python::False(py).into_object()
            }
        }
        Yaml::String(string) => {
            string.into_py_object(py).into_object()
        }
        Yaml::Array(array) => {
            let vec: Vec<_> = array.iter().map(|data| yaml_to_python(data, py)).collect();
            cpython::PyList::new(py, &vec).into_object()

        }
        Yaml::Hash(hash) => {
            let dict = PyDict::new(py);
            for (key, value) in hash.iter() {
                dict.set_item(py, yaml_to_python(key,py), yaml_to_python(value, py)).unwrap();
            }
            
            dict.into_object()

        }
        Yaml::Null => {
            cpython::Python::None(py)
        }
        Yaml::BadValue => {
            panic!("Received a BadValue");
        }
        Yaml::Alias(index) => { // Alias are not supported right now, they are insecure and problematic anyway
            format!("Alias, index: {}", index).into_py_object(py).into_object()
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

fn python_init() {
    // Add this executable as a Python module
    let mod_name = std::ffi::CString::new("lily").unwrap();
    unsafe {assert!(python3_sys::PyImport_AppendInittab(mod_name.into_raw(), Some(PyInit_lily)) != -1);};
}

fn main() {
    init_log();
    python_init();
    record_loop();
}