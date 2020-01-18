// Standard library
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;
use core::cell::RefCell;
use std::collections::HashMap;

// This crate
use crate::python::{yaml_to_python, add_to_sys_path, PYTHON_LILY_PKG_NONE};

// Other crates
use unic_langid::LanguageIdentifier;
use cpython::{Python, PyTuple, PyDict, ObjectProtocol, PyClone};
use log::info;
use ref_thread_local::RefThreadLocal;

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

    pub fn new_with_no_trans(py: Python, actions_path: &Path) -> Self {
        let mut reg = Self{map: HashMap::new()};
        reg.add_folder_no_trans(py, actions_path);
        reg
    }

    pub fn add_folder(&mut self,python: Python, actions_path: &Path, curr_lang: &LanguageIdentifier) {
        self.add_folder_no_trans(python, actions_path);

        let lily_py_mod = python.import("lily_ext").unwrap();

        // The path is the Python path, so set the current directory to it's parent (the package)
        let canon_path = actions_path.parent().unwrap().canonicalize().unwrap();
        info!("Actions_path:{}", canon_path.to_str().unwrap());
        std::env::set_current_dir(canon_path).unwrap();
        lily_py_mod.call(python, "__set_translations", (curr_lang.to_string(),), None).unwrap();
    }

    pub fn add_folder_no_trans(&mut self, python: Python, actions_path: &Path) {

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
        
        for (key, val) in lily_py_mod.get(python, "action_classes").unwrap().cast_into::<PyDict>(python).unwrap().items(python) {
            self.map.insert(key.to_string(), val.clone_ref(python));
            println!("{:?}:{:?}", key.to_string(), val.to_string());
        }
    }

    pub fn clone_adding(&self, py: Python, new_actions_path: &Path, curr_lang: &LanguageIdentifier) -> Self {
        let mut new = self.clone();
        new.add_folder(py, new_actions_path, curr_lang);
        new
    }

    pub fn clone_try_adding(&self, py: Python, new_actions_path: &Path, curr_lang: &LanguageIdentifier) -> Self {
        if new_actions_path.is_dir() {
            self.clone_adding(py, new_actions_path, curr_lang)
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
    args: cpython::PyObject,
    lily_pkg_name: Rc<String>,
    lily_pkg_path: Rc<PathBuf>
}

pub struct ActionSet {
    acts: Vec<ActionData>
}

impl ActionSet {
    pub fn create() -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {acts: Vec::new()}))
    }
    pub fn add_action(&mut self, py: Python, act_name: &str, yaml: &yaml_rust::Yaml, action_registry: &ActionRegistry, lily_pkg_name: Rc<String>, lily_pkg_path: Rc<PathBuf>) {
        self.acts.push(ActionData{obj: action_registry.get(act_name).unwrap().clone_ref(py), args: yaml_to_python(&yaml, py), lily_pkg_name, lily_pkg_path});
    }
    pub fn call_all(&mut self, py: Python) {
        for action in self.acts.iter() {
            let trig_act = action.obj.getattr(py, "trigger_action").unwrap();
            std::env::set_current_dir(*action.lily_pkg_path);
            *crate::python::PYTHON_LILY_PKG_CURR.borrow_mut() = action.lily_pkg_name.clone();
            trig_act.call(py, PyTuple::new(py, &[action.args.clone_ref(py)]), None).unwrap();
            *crate::python::PYTHON_LILY_PKG_CURR.borrow_mut() = PYTHON_LILY_PKG_NONE.borrow().clone();
        }
    }
}

pub struct OrderMap {
    map: HashMap<String, Rc<RefCell<ActionSet>>>
}

impl OrderMap {
    pub fn new() -> Self {
        Self{map: HashMap::new()}
    }

    pub fn add_order(&mut self, order_name: &str, act_set: Rc<RefCell<ActionSet>>) {
        let action_entry = self.map.entry(order_name.to_string()).or_insert(ActionSet::create());
        *action_entry = act_set;
    }

    pub fn call_order(&mut self, act_name: &str) {
        if let Some(action_set) = self.map.get_mut(act_name) {
            let gil = Python::acquire_gil();
            let python = gil.python();

            action_set.borrow_mut().call_all(python);
        }
    }
}