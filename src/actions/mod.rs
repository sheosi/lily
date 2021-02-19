mod action_context;

pub use self::action_context::*;

// Standard library
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt; // For Debug in LocalActionRegistry
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

// This crate
use crate::collections::{BaseRegistry, GlobalReg, LocalBaseRegistry};
use crate::skills::call_for_skill;
use crate::python::{get_inst_class_name, HalfBakedError, PyException};

// Other crates
use anyhow::{anyhow, Result};
use log::{error, warn};
use pyo3::{Py, PyAny, PyObject, Python, types::PyTuple};
use pyo3::prelude::{pyclass, pymethods};
use pyo3::exceptions::PyOSError;

pub type ActionRegistryShared = Rc<RefCell<ActionRegistry>>;

pub trait ActionInstance {
    fn call(&self, context: &ActionContext) -> Result<()>;
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
        action_classes: Vec<(PyObject, PyObject)>)
        -> Result<(), HalfBakedError> {

        let actions = Self::extend_and_init_classes(&mut act_reg.get_global_mut(), py, action_classes)?;
        act_reg.extend_with_map(actions);
        Ok(())
    }

    // Imports all modules from that module and return the new actions
    fn extend_and_init_classes(
        act_reg: &mut ActionRegistry,
        python: Python,
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
                    act_reg.insert(name, action);
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
    fn call(&self ,context: &ActionContext) -> Result<()> {
        let gil = Python::acquire_gil();
        let py = gil.python();

        let trig_act = self.obj.getattr(py, "trigger_action")?;
        std::env::set_current_dir(self.lily_skill_path.as_ref())?;
        call_for_skill(self.lily_skill_path.as_ref(),
        |_|trig_act.call(
            py,
            (context.clone(),),
            None)
        ).py_excep::<PyOSError>()??;

        Ok(())
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

    pub fn call_all(&mut self, context: &ActionContext) {
        for action in &self.acts {
            if let Err(e) = action.call(context) {
                error!("Action {} failed while being triggered: {}", &action.get_name(), e);
            }
        }
    }
}

pub trait SharedActionSet {
    fn call_all<F: FnOnce()->String>(&self, context: &ActionContext, f: F);
}
impl SharedActionSet for Arc<Mutex<ActionSet>> {
    fn call_all<F: FnOnce()->String>(&self, context: &ActionContext, f: F) {
        match self.lock() {
            Ok(ref mut m) => {
                m.call_all(context);
            }
            Err(_) => {
                warn!("ActionSet of  {} had an error before and can't be used anymore", f());
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
    fn call(&mut self, context: &ActionContext) {
        
        match self.act_set.lock() {
            Ok(ref mut m) => m.call_all(context),
            Err(_) => warn!("A PyActionSet had an error before and can't be used anymore")
        }
    }
}