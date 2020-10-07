// Standard library
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

// This crate
use crate::python::{call_for_pkg, get_inst_class_name, PyException, yaml_to_python};

// Other crates
use anyhow::{anyhow, Result};
use log::error;
use pyo3::{types::{PyDict,PyTuple}, PyObject, Python};
use pyo3::prelude::{pyclass, pymethods};
use pyo3::exceptions::PyOSError;

type ActionRegistryShared = Rc<RefCell<ActionRegistry>>;

#[derive(Debug)]
pub struct ActionRegistry {
    map: HashMap<String, PyObject>
}

impl ActionRegistry {

    pub fn new() -> Self {
        Self{map: HashMap::new()}
    }

    // Imports all modules from that module and return the new actions
    pub fn extend_and_init_classes(&mut self, python: Python, action_classes: Vec<(PyObject, PyObject)>) -> Result<HashMap<String, PyObject>> {
        let mut res = HashMap::new();

        for (key, val) in  &action_classes {
            let name = key.to_string();
            // We'll get old items, let's ignore them
            if !self.map.contains_key(&name) {
                let pyobj = val.call(python, PyTuple::empty(python), None).map_err(|py_err|anyhow!("Python error while instancing action \"{}\": {:?}", name, py_err))?;
                res.insert(name.clone(), pyobj.clone_ref(python));
                self.map.insert(name, pyobj);
            }
        }

        Ok(res)
    }
}

impl Clone for LocalActionRegistry {
    fn clone(&self) -> Self {
        // Get GIL
        let gil = Python::acquire_gil();
        let python = gil.python();

        let dup_refs = |pair:(&String, &PyObject)| {
            let (key, val) = pair;
            (key.to_owned(), val.clone_ref(python))
        };

        let new_map: HashMap<String, PyObject> = self.map.iter().map(dup_refs).collect();
        Self{map: new_map, global_reg: self.global_reg.clone()}
    }
}


#[derive(Debug)]
pub struct LocalActionRegistry {
    map: HashMap<String, PyObject>,
    global_reg: Rc<RefCell<ActionRegistry>>
}

impl LocalActionRegistry {
    pub fn new(global_reg: ActionRegistryShared) -> Self {
        Self {map: HashMap::new(), global_reg}
    }

    pub fn extend_and_init_classes(&mut self, py:Python, action_classes: Vec<(PyObject, PyObject)>) -> Result<()> {
        self.map.extend( (*self.global_reg).borrow_mut().extend_and_init_classes(py, action_classes)?);
        Ok(())
    }

    fn get(&self, action_name: &str) -> Option<&PyObject> {
        self.map.get(action_name)
    }
}

struct ActionData {
    obj: PyObject,
    args: PyObject,
    lily_pkg_path: Arc<PathBuf>
}

pub struct ActionSet {
    acts: Vec<ActionData>
}

impl ActionSet {
    pub fn create() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {acts: Vec::new()}))
    }

    pub fn add_action(&mut self, py: Python, act_name: &str, yaml: &serde_yaml::Value, action_registry: &LocalActionRegistry, lily_pkg_path: Arc<PathBuf>) -> Result<()>{
        let act_obj = action_registry.get(act_name).ok_or_else(||anyhow!("Action {} is not registered",act_name))?.clone_ref(py);
        self.acts.push(ActionData{obj: act_obj, args: yaml_to_python(py, &yaml), lily_pkg_path});

        Ok(())
    }

    pub fn call_all(&mut self, py: Python, context: &PyDict) {
        fn call_action(py: Python, action: &ActionData, context: &PyDict) -> Result<()> {
            let trig_act = action.obj.getattr(py, "trigger_action")?;
            std::env::set_current_dir(action.lily_pkg_path.as_ref())?;
            call_for_pkg(action.lily_pkg_path.as_ref(),
            |_|trig_act.call(
                py,
                (action.args.clone_ref(py), context),
                None)
            ).py_excep::<PyOSError>()??;

            Ok(())
        }

        for action in &self.acts {
            if let Err(e) = call_action(py, action, context) {
                let name = get_inst_class_name(py, &action.obj);
                error!("Action {} failed while being triggered: {}", name, e);
            }
        }
    }
}

#[pyclass]
pub struct PyActionSet {
    act_set: Arc<Mutex<ActionSet>>
}

impl PyActionSet {
    pub fn from_arc(act_set: Arc<Mutex<ActionSet>>) -> Self {
        Self {act_set}
    }
}

#[pymethods]
impl PyActionSet {
    fn call(&mut self, py: Python, context: &PyDict) {
        self.act_set.lock().unwrap().call_all(py, context)
    }
}