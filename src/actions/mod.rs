mod action_context;

pub use self::action_context::*;

// Standard library
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt; // For Debug in LocalActionRegistry
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

// This crate
use crate::collections::{BaseRegistry, GlobalReg, LocalBaseRegistry};
use crate::skills::call_for_skill;
use crate::python::{get_inst_class_name, HalfBakedError, PyException};
use crate::vars::POISON_MSG;

// Other crates
use anyhow::{anyhow, Result};
use log::error;
use lily_common::audio::Audio;
use pyo3::{Py, PyAny, PyObject, PyResult, Python, types::PyTuple};
use pyo3::prelude::{pyclass, pymethods};
use pyo3::exceptions::PyOSError;

pub type ActionRegistryShared = Rc<RefCell<ActionRegistry>>;

#[derive(Clone)]
pub enum MainAnswer {
    Sound(Audio),
    Text(String)
}

#[pyclass]
#[derive(Clone)]
pub struct ActionAnswer {
    pub answer: MainAnswer
}


impl ActionAnswer {
    pub fn audio_file(path: &Path) -> Result<Self> {
        let mut f = File::open(path)?;
        let mut buffer = vec![0; fs::metadata(path)?.len() as usize];
        f.read(&mut buffer)?;
        let a = Audio::new_encoded(buffer);
        Ok(Self {answer: MainAnswer::Sound(a)})
    }

    pub fn send_text(text: String) -> Result<Self> {
        Ok(Self {answer: MainAnswer::Text(text)})
    }
}

#[pymethods]
impl ActionAnswer {
    #[staticmethod]
    pub fn load_audio(path: &str) -> PyResult<Self> {
        Self::audio_file(Path::new(path)).py_excep::<PyOSError>()
    }
    #[staticmethod]
    pub fn text(text: String) -> PyResult<Self> {
        Self::send_text(text).py_excep::<PyOSError>()
    }
}


pub trait ActionInstance {
    fn call(&self, context: &ActionContext) -> Result<ActionAnswer>;
    fn get_name(&self) -> String;
}

pub trait Action {
    fn instance(&self, lily_skill_path:Arc<PathBuf>) -> Box<dyn ActionInstance + Send>;
}


pub type ActionRegistry = BaseRegistry<dyn Action + Send>;
pub type LocalActionRegistry = LocalBaseRegistry<dyn Action + Send, ActionRegistry>;


#[derive(Debug)]
pub struct PythonAction {
    act_name: Py<PyAny>,
    obj: PyObject
}

impl PythonAction {
    pub fn new(act_name: Py<PyAny>, obj: PyObject) -> Self {
        Self{act_name, obj}
    }

    pub fn extend_and_init_classes_local(
        act_reg: &mut LocalActionRegistry,
        py:Python,
        skill_name: String,
        action_classes: Vec<(PyObject, PyObject)>)
        -> Result<(), HalfBakedError> {

        let actions = Self::extend_and_init_classes(&mut act_reg.get_global_mut(), py, skill_name, action_classes)?;
        act_reg.extend_with_map(actions);
        Ok(())
    }

    // Imports all modules from that module and return the new actions
    fn extend_and_init_classes(
        act_reg: &mut ActionRegistry,
        python: Python,
        skill_name: String,
        action_classes: Vec<(PyObject, PyObject)>)
        -> Result<HashMap<String, Rc<RefCell<dyn Action + Send>>>, HalfBakedError> {

        let process_list = || -> Result<_> {
            let mut act_to_add = vec![];
            for (key, val) in  &action_classes {
                let name = key.to_string();
                let pyobj = val.call(python, PyTuple::empty(python), None).map_err(|py_err|anyhow!("Python error while instancing action \"{}\": {:?}", name, py_err.to_string()))?;
                let rc: Rc<RefCell<dyn Action + Send>> = Rc::new(RefCell::new(PythonAction::new(key.to_owned(),pyobj)));
                act_to_add.push((name,rc));
            }
            Ok(act_to_add)
        };

        match process_list() {
            Ok(act_to_add) => {
                let mut res = HashMap::new();

                for (name, action) in act_to_add {
                    res.insert(name.clone(), action.clone());
                    if let Err (e) = act_reg.insert(skill_name.clone(),name, action) {
                        error!("{}", e);
                    }
                }

                Ok(res)
            }

            Err(e) => {
                Err(HalfBakedError::from(
                    HalfBakedError::gen_diff(&act_reg.get_map_ref(), action_classes),
                    e
                ))
            }
        }
    }
}

impl Action for PythonAction {
    fn instance(&self, lily_skill_path:Arc<PathBuf>) -> Box<dyn ActionInstance + Send> {
        let gil = Python::acquire_gil();
        let py = gil.python();
        Box::new(PythonActionInstance::new(py, self.obj.clone(), lily_skill_path))
    }
}


pub struct PythonActionInstance {
    obj: PyObject,
    lily_skill_path: Arc<PathBuf>
}

impl PythonActionInstance {
    pub fn new (py: Python, act_obj:Py<PyAny>, lily_skill_path:Arc<PathBuf>) -> Self {
        Self{obj: act_obj, lily_skill_path}
    }
}

impl ActionInstance for PythonActionInstance {
    fn call(&self ,context: &ActionContext) -> Result<ActionAnswer> {
        let gil = Python::acquire_gil();
        let py = gil.python();

        let trig_act = self.obj.getattr(py, "trigger_action")?;
        std::env::set_current_dir(self.lily_skill_path.as_ref())?;
        let r = call_for_skill(self.lily_skill_path.as_ref(),
        |_|trig_act.call(
            py,
            (context.clone(),),
            None)
        ).py_excep::<PyOSError>()??;

        Ok(r.extract(py)?)
    }
    fn get_name(&self) -> String {
        let gil = Python::acquire_gil();
        let py = gil.python();

        get_inst_class_name(py, &self.obj).expect("Python object has not class name")
    }
}


pub struct ActionSet {
    acts: Vec<Box<dyn ActionInstance + Send>>
}

impl fmt::Debug for ActionSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ActionRegistry")
         .field("acts", &self.acts.iter().fold("".to_string(), |str, a|format!("{}{},",str,a.get_name())))
         .finish()
    }
}

impl ActionSet {
    pub fn create() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {acts: Vec::new()}))
    }

    pub fn with(action: Box<dyn ActionInstance + Send>) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {acts: vec![action]}))
    }

    pub fn add_action(&mut self, action: Box<dyn ActionInstance + Send>) -> Result<()>{
        self.acts.push(action);

        Ok(())
    }

    pub fn call_all(&mut self, context: &ActionContext) -> Vec<ActionAnswer> {
        let mut res = Vec::new();
        for action in &self.acts {
            match action.call(context) {
                Ok(a) => res.push(a),
                Err(e) =>  {
                    error!("Action {} failed while being triggered: {}", &action.get_name(), e);
                }
            }
        }
        res
    }
}

// Just exists as a way of extending Arc
pub trait SharedActionSet {
    fn call_all(&self, context: &ActionContext) -> Vec<ActionAnswer>;
}
impl SharedActionSet for Arc<Mutex<ActionSet>> {
    fn call_all(&self, context: &ActionContext) -> Vec<ActionAnswer> {
        self.lock().expect(POISON_MSG).call_all(context)
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
    fn call(&mut self, context: &ActionContext) -> Vec<ActionAnswer> {
        self.act_set.lock().expect(POISON_MSG).call_all(context)
    }
}