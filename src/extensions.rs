// Standard library
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;
use core::cell::RefCell;
use std::collections::HashMap;

// This crate
use crate::python::{yaml_to_python, add_to_sys_path, PYTHON_LILY_PKG_NONE, PYTHON_LILY_PKG_CURR};

// Other crates
use unic_langid::LanguageIdentifier;
use cpython::{Python, PyTuple, PyDict, ObjectProtocol, PyClone};
use log::info;
use ref_thread_local::RefThreadLocal;
use anyhow::{anyhow, Result};

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

    pub fn new_with_no_trans(py: Python, actions_path: &Path) -> Result<Self> {
        let mut reg = Self{map: HashMap::new()};
        reg.add_folder_no_trans(py, actions_path)?;
        Ok(reg)
    }

    pub fn add_folder(&mut self,python: Python, actions_path: &Path, curr_lang: &LanguageIdentifier) -> Result<()> {
        self.add_folder_no_trans(python, actions_path)?;

        let lily_py_mod = python.import("lily_ext").map_err(|py_err|anyhow!("Python error while importing lily_ext: {:?}", py_err))?;

        // The path is the Python path, so set the current directory to it's parent (the package)
        let canon_path = actions_path.parent().ok_or_else(||anyhow!("Can't get parent for the action's path"))?.canonicalize()?;
        info!("Actions_path:{}", canon_path.to_str().ok_or_else(||anyhow!("Can't transform action's path name to str"))?);
        let pkg_name = {
            let os_str = canon_path.file_name().ok_or_else(||anyhow!("Can't get package path's name"))?;
            let pkg_name_str = os_str.to_str().ok_or_else(||anyhow!("Can't transform package path name to str"))?;
            Rc::new(pkg_name_str.to_string())
        };

        std::env::set_current_dir(canon_path)?;

        
        *PYTHON_LILY_PKG_CURR.borrow_mut() = pkg_name;
        lily_py_mod.call(python, "__set_translations", (curr_lang.to_string(),), None).map_err(|py_err|anyhow!("Python error while calling __set_translations: {:?}", py_err))?;
        *PYTHON_LILY_PKG_CURR.borrow_mut() = crate::python::PYTHON_LILY_PKG_NONE.borrow().clone();

        Ok(())
    }

    pub fn add_folder_no_trans(&mut self, python: Python, actions_path: &Path) -> Result<()> {

        // Add folder to sys.path
        add_to_sys_path(python, actions_path).map_err(|py_err|anyhow!("Python error while adding to sys.path: {:?}", py_err))?;
        info!("Add folder: {}", actions_path.to_str().ok_or_else(||anyhow!("Coudln't transform actions_path into string"))?);

        // Make order_map from python's modules
        for entry in std::fs::read_dir(actions_path)? {
            let entry = entry?;
            if entry.path().is_dir() {
                let mod_name = entry.file_name().into_string().map_err(|os_str|anyhow!("Failed to transform module name into Unicode: {:?}", os_str))?.to_string();
                python.import(&mod_name).map_err(|py_err|anyhow!("Failed to import a package's python module: {:?}", py_err))?;
            }
        }
        
        let action_classes = {
            let lily_py_mod = python.import("lily_ext").map_err(|py_err|anyhow!("Python error while importing lily_ext: {:?}", py_err))?;
            let act_cls_obj = lily_py_mod.get(python, "action_classes").map_err(|py_err|anyhow!("Python error while obtaining action_classes: {:?}", py_err))?;
            let act_cls_dict = act_cls_obj.cast_into::<PyDict>(python).map_err(|py_err|anyhow!("Python error while casting action_classes into PyDict: {:?}", py_err))?;
            act_cls_dict.items(python)
        };

        for (key, val) in  action_classes {
            self.map.insert(key.to_string(), val.clone_ref(python));
            println!("{:?}:{:?}", key.to_string(), val.to_string());
        }
        Ok(())
    }

    pub fn clone_adding(&self, py: Python, new_actions_path: &Path, curr_lang: &LanguageIdentifier) -> Result<Self> {
        let mut new = self.clone();
        new.add_folder(py, new_actions_path, curr_lang)?;
        Ok(new)
    }

    pub fn clone_try_adding(&self, py: Python, new_actions_path: &Path, curr_lang: &LanguageIdentifier) -> Result<Self> {
        if new_actions_path.is_dir() {
            self.clone_adding(py, new_actions_path, curr_lang)
        }
        else {
            Ok(self.clone())
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
    pub fn add_action(&mut self, py: Python, act_name: &str, yaml: &yaml_rust::Yaml, action_registry: &ActionRegistry, lily_pkg_name: Rc<String>, lily_pkg_path: Rc<PathBuf>) -> Result<()>{
        let act_obj = action_registry.get(act_name).ok_or_else(||anyhow!("Action {} is not registered",act_name))?.clone_ref(py);
        self.acts.push(ActionData{obj: act_obj, args: yaml_to_python(&yaml, py), lily_pkg_name, lily_pkg_path});

        Ok(())
    }
    pub fn call_all(&mut self, py: Python) -> Result<()> {
        for action in self.acts.iter() {
            // TODO: if an object doesn't implement trigger_action, don't panic, but decide what to do
            let trig_act = action.obj.getattr(py, "trigger_action").map_err(|py_err|anyhow!("Python error while accessing trigger_action: {:?}", py_err))?; 
            std::env::set_current_dir(action.lily_pkg_path.as_ref())?;
            *crate::python::PYTHON_LILY_PKG_CURR.borrow_mut() = action.lily_pkg_name.clone();
            trig_act.call(py, PyTuple::new(py, &[action.args.clone_ref(py)]), None).map_err(|py_err|{py_err.clone_ref(py).print(py);anyhow!("Python error while calling action: {:?}", py_err)})?;
            *crate::python::PYTHON_LILY_PKG_CURR.borrow_mut() = PYTHON_LILY_PKG_NONE.borrow().clone();
        }

        Ok(())
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

    pub fn call_order(&mut self, act_name: &str) -> Result<()> {
        if let Some(action_set) = self.map.get_mut(act_name) {
            let gil = Python::acquire_gil();
            let python = gil.python();

            action_set.borrow_mut().call_all(python)?;
        }

        Ok(())
    }
}