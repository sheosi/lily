// Standard library
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

// This crate
use crate::python::{call_for_pkg, get_inst_class_name, HalfBakedError, PyException, yaml_to_python};

// Other crates
use anyhow::{anyhow, Result};
use log::error;
use pyo3::{types::{PyDict,PyTuple}, Py, PyAny, PyObject, Python};
use pyo3::prelude::{pyclass, pymethods};
use pyo3::exceptions::PyOSError;

type ActionRegistryShared = Rc<RefCell<ActionRegistry>>;

pub trait ActionInstance {
    fn call(&self, py: Python, context: &PyDict) -> Result<()>;
    fn get_name(&self) -> String;
}

pub trait Action {
    fn instance(&self, py: Python, yaml: &serde_yaml::Value, lily_pkg_path:Arc<PathBuf>) -> Box<dyn ActionInstance + Send>;
}


pub struct ActionRegistry {
    map: HashMap<String, Rc<RefCell<dyn Action + Send>>>
}

impl ActionRegistry {

    pub fn new() -> Self {
        Self{map: HashMap::new()}
    }

    // Imports all modules from that module and return the new actions
    pub fn extend_and_init_classes(&mut self, python: Python, action_classes: Vec<(PyObject, PyObject)>) -> Result<HashMap<String, Rc<RefCell<dyn Action + Send>>>, HalfBakedError> {

        let process_list = || -> Result<_> {
            let mut act_to_add = vec![];
            for (key, val) in  &action_classes {
                let name = key.to_string();
                // We'll get old items, let's ignore them
                if !self.map.contains_key(&name) {
                    let pyobj = val.call(python, PyTuple::empty(python), None).map_err(|py_err|anyhow!("Python error while instancing action \"{}\": {:?}", name, py_err.to_string()))?;
                    let rc: Rc<RefCell<dyn Action + Send>> = Rc::new(RefCell::new(PythonAction::new(pyobj)));
                    act_to_add.push((name,rc));
                }
            }
            Ok(act_to_add)
        };

        match process_list() {
            Ok(act_to_add) => {
                let mut res = HashMap::new();

                for (name, action) in act_to_add {
                    res.insert(name.clone(), action.clone());
                    self.map.insert(name, action);
                }

                Ok(res)
            }

            Err(e) => {
                Err(HalfBakedError::from(
                    HalfBakedError::gen_diff(&self.map, action_classes),
                    e
                ))
            }
        }
    }
}

pub struct LocalActionRegistry {
    map: HashMap<String, Rc<RefCell<dyn Action + Send>>>,
    global_reg: Rc<RefCell<ActionRegistry>>
}

impl LocalActionRegistry {
    pub fn new(global_reg: ActionRegistryShared) -> Self {
        Self {map: HashMap::new(), global_reg}
    }

    pub fn extend_and_init_classes(&mut self, py:Python, action_classes: Vec<(PyObject, PyObject)>) -> Result<(), HalfBakedError> {
        self.map.extend( (*self.global_reg).borrow_mut().extend_and_init_classes(py, action_classes)?);
        Ok(())
    }

    pub fn get(&self, action_name: &str) -> Option<&Rc<RefCell<dyn Action + Send>>> {
        self.map.get(action_name)
    }
}

impl Clone for LocalActionRegistry {
    fn clone(&self) -> Self {        
        let dup_refs = |pair:(&String, &Rc<RefCell<dyn Action + Send>>)| {
            let (key, val) = pair;
            (key.to_owned(), val.clone())
        };

        let new_map: HashMap<String, Rc<RefCell<dyn Action + Send>>> = self.map.iter().map(dup_refs).collect();
        Self{map: new_map, global_reg: self.global_reg.clone()}
    }
}

#[derive(Debug)]
pub struct PythonAction {
    obj: PyObject
}

impl PythonAction {
    pub fn new(obj: PyObject) -> Self {
        Self{obj}
    }
}

impl Action for PythonAction {
    fn instance(&self, py: Python, yaml: &serde_yaml::Value, lily_pkg_path:Arc<PathBuf>) -> Box<dyn ActionInstance + Send> {
        Box::new(PythonActionInstance::new(py, self.obj.clone(), yaml, lily_pkg_path))
    }
}

pub struct PythonActionInstance {
    obj: PyObject,
    args: PyObject,
    lily_pkg_path: Arc<PathBuf>
}

impl PythonActionInstance {
    pub fn new (py: Python, act_obj:Py<PyAny>, yaml: &serde_yaml::Value, lily_pkg_path:Arc<PathBuf>) -> Self {
        Self{obj: act_obj, args: yaml_to_python(py, &yaml), lily_pkg_path}
    }
}

impl ActionInstance for PythonActionInstance {
    fn call(&self ,py: Python, context: &PyDict) -> Result<()> {
        let trig_act = self.obj.getattr(py, "trigger_action")?;
        std::env::set_current_dir(self.lily_pkg_path.as_ref())?;
        call_for_pkg(self.lily_pkg_path.as_ref(),
        |_|trig_act.call(
            py,
            (self.args.clone_ref(py), context),
            None)
        ).py_excep::<PyOSError>()??;

        Ok(())
    }
    fn get_name(&self) -> String {
        let gil = Python::acquire_gil();
        let py = gil.python();

        get_inst_class_name(py, &self.obj)
    }
}

pub struct ActionSet {
    acts: Vec<Box<dyn ActionInstance + Send>>
}

impl ActionSet {
    pub fn create() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {acts: Vec::new()}))
    }

    pub fn add_action(&mut self, action: Box<dyn ActionInstance + Send>) -> Result<()>{
        self.acts.push(action);

        Ok(())
    }

    pub fn call_all(&mut self, py: Python, context: &PyDict) {
        for action in &self.acts {
            if let Err(e) = action.call(py, context) {
                error!("Action {} failed while being triggered: {}", &action.get_name(), e);
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