mod action_context;

pub use self::action_context::*;

// Standard library
use std::fmt; // For Debug in LocalActionRegistry
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};

// This crate
use crate::collections::{BaseRegistry, GlobalReg};
use crate::exts::LockIt;
use crate::skills::call_for_skill;
#[cfg(feature="python_skills")]
use crate::python::{get_inst_class_name, HalfBakedError, PyException};

// Other crates
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::executor::block_on;
use lazy_static::lazy_static;
use log::error;
use lily_common::audio::Audio;
#[cfg(feature="python_skills")]
use pyo3::{Py, PyAny, PyObject, PyResult, Python, types::PyTuple};
#[cfg(feature="python_skills")]
use pyo3::prelude::{pyclass, pymethods};
#[cfg(feature="python_skills")]
use pyo3::exceptions::PyOSError;
use tokio::runtime::Handle;

pub type ActionRegistry = BaseRegistry<dyn Action + Send>;
pub type ActionItem = Arc<Mutex<dyn Action + Send>>;

lazy_static! {
    pub static ref ACT_REG: Mutex<ActionRegistry> = Mutex::new(ActionRegistry::new());
}

#[derive(Clone)]
pub enum MainAnswer {
    Sound(Audio),
    Text(String)
}

#[cfg(feature="python_skills")]
#[pyclass]
#[derive(Clone)]
pub struct ActionAnswer {
    pub answer: MainAnswer,
    pub should_end_session: bool
}

#[cfg(not(feature="python_skills"))]
#[derive(Clone)]
pub struct ActionAnswer {
    pub answer: MainAnswer,
    pub should_end_session: bool
}

impl ActionAnswer {
    pub fn audio_file(path: &Path, end_session: bool) -> Result<Self> {
        let mut f = File::open(path)?;
        let mut buffer = vec![0; fs::metadata(path)?.len() as usize];
        f.read(&mut buffer)?;
        let a = Audio::new_encoded(buffer);
        Ok(Self {answer: MainAnswer::Sound(a), should_end_session: end_session})
    }

    pub fn send_text(text: String, end_session: bool) -> Result<Self> {
        Ok(Self {answer: MainAnswer::Text(text), should_end_session: end_session})
    }
}

#[cfg(feature="python_skills")]
#[pymethods]
impl ActionAnswer {
    #[staticmethod]
    #[args(end_session="true")]
    pub fn load_audio(path: &str, end_session: bool) -> PyResult<Self> {
        Self::audio_file(Path::new(path), end_session).py_excep::<PyOSError>()
    }
    #[staticmethod]
    #[args(end_session="true")]
    pub fn text(text: String, end_session: bool) -> PyResult<Self> {
        Self::send_text(text, end_session).py_excep::<PyOSError>()
    }
}

#[async_trait(?Send)]
pub trait Action {
    async fn call(&self, context: &ActionContext) -> Result<ActionAnswer>;
    fn get_name(&self) -> String;
}

pub trait ActionItemExt {
    fn new<A: Action + Send + 'static>(act: A) -> ActionItem;
}

impl ActionItemExt for ActionItem {
    fn new<A: Action + Send + 'static>(act: A) -> ActionItem {
        Arc::new(Mutex::new(act))
    }
}

#[cfg(feature="python_skills")]
#[derive(Debug)]
pub struct PythonAction {
    act_name: Py<PyAny>,
    obj: PyObject, // this is the class
    skill_path:Arc<PathBuf>
}

#[cfg(feature="python_skills")]
impl PythonAction {

    // Old PythonAction
    pub fn new(act_name: Py<PyAny>, obj: PyObject, skill_path: Arc<PathBuf>) -> Self {
        Self{act_name, obj, skill_path}
    }

    // Imports all modules from that module and return the new actions
    pub fn extend_and_init_classes(
        python: Python,
        skill_name: String,
        action_classes: Vec<(PyObject, PyObject)>,
        skill_path: Arc<PathBuf>) -> Result<Vec<String>, HalfBakedError> {
        
        let process_list = || -> Result<_> {
            let mut act_to_add = vec![];
            for (key, val) in  &action_classes {
                let name = key.to_string();
                let pyobj = val.call(python, PyTuple::empty(python), None).map_err(|py_err|anyhow!("Python error while instancing action \"{}\": {:?}", name, py_err.to_string()))?;
                let rc: ActionItem = Arc::new(Mutex::new(PythonAction::new(key.to_owned(),pyobj, skill_path.clone())));
                act_to_add.push((name,rc));
            }
            Ok(act_to_add)
        };

        match process_list() {
            Ok(act_to_add) => {
                let acts = act_to_add.iter().map(|(n, _)| n.clone()).collect();
                for (name, action) in act_to_add {
                    if let Err (e) = ACT_REG.lock_it().insert(&skill_name,&name, action) {
                        error!("{}", e);
                    }
                }

                Ok(acts)
            }

            Err(e) => {
                Err(HalfBakedError::from(
                    HalfBakedError::gen_diff(ACT_REG.lock_it().get_map_ref(), action_classes),
                    e
                ))
            }
        }
    }
}

#[cfg(feature="python_skills")]
#[async_trait(?Send)]
impl Action for PythonAction {
    async fn call(&self ,context: &ActionContext) -> Result<ActionAnswer> {
        let gil = Python::acquire_gil();
        let py = gil.python();

        let trig_act = self.obj.getattr(py, "trigger_action")?;
        std::env::set_current_dir(self.skill_path.as_ref())?;
        let r = call_for_skill(self.skill_path.as_ref(),
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

#[derive(Clone)]
pub struct ActionSet {
    acts: Vec<Weak<Mutex<dyn Action + Send>>>
}

impl fmt::Debug for ActionSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ActionRegistry")
         .field("acts", &self.acts.iter().fold("".to_string(), |str, a|format!("{}{},",str,a.upgrade().unwrap().lock_it().get_name())))
         .finish()
    }
}

impl ActionSet {
    pub fn create(a: Weak<Mutex<dyn Action + Send>>) -> Self {
        Self {acts: vec![a]}
    }

    pub fn empty() -> Self {
        Self {acts: vec![]}
    }

    pub fn add_action(&mut self, action: Weak<Mutex<dyn Action + Send>>) {
        self.acts.push(action);
    }

    pub async fn call_all(&self, context: &ActionContext) -> Vec<ActionAnswer> {
        let mut res = Vec::new();
        for action in &self.acts {
            match action.upgrade().unwrap().lock_it().call(context).await {
                Ok(a) => res.push(a),
                Err(e) =>  {
                    error!("Action {} failed while being triggered: {}", &action.upgrade().unwrap().lock_it().get_name(), e);
                }
            }
        }
        res
    }
}

#[cfg(feature="python_skills")]
#[pyclass]
pub struct PyActionSet {
    act_set: ActionSet
}

#[cfg(feature="python_skills")]
impl PyActionSet {
    pub fn from_set(act_set: ActionSet) -> Self {
        Self {act_set}
    }
}

#[cfg(feature="python_skills")]
#[pymethods]
impl PyActionSet {
    fn call(&mut self, context: &ActionContext) -> Vec<ActionAnswer> {
        let handle = Handle::current();
        let _enter_grd= handle.enter();
        block_on(self.act_set.call_all(context))
    }
}

// Just a sample action for testing
pub struct SayHelloAction {}

impl SayHelloAction {
    pub fn new() -> Self {
        SayHelloAction{}
    }
}

#[async_trait(?Send)]
impl Action for SayHelloAction {
    async fn call(&self, _context: &ActionContext) -> Result<ActionAnswer> {
        ActionAnswer::send_text("Hello".into(), true)
    }

    fn get_name(&self) -> String {
        "say_hello".into()
    }
}