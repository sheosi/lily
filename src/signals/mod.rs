pub mod order;

pub use self::order::*;

// Standard library
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

// This crate
use crate::actions::{ActionSet, PyActionSet};
use crate::config::Config;
use crate::python::{call_for_pkg, HalfBakedError, remove_from_signals, yaml_to_python};

// Other crates
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use pyo3::{conversion::IntoPy, types::{PyDict, PyTuple}, Py, PyAny, Python, PyObject};
use lily_common::extensions::MakeSendable;
use log::warn;
use unic_langid::LanguageIdentifier;

pub type SignalEventShared = Arc<Mutex<SignalEvent>>;
type SignalRegistryShared = Rc<RefCell<SignalRegistry>>;

// A especial signal to be called by the system whenever something happens
pub struct SignalEvent {
    event_map: OrderMap
}

impl SignalEvent {
    pub fn new() -> Self {
        Self {event_map: OrderMap::new()}
    }

    pub fn add(&mut self, event_name: &str, act_set: Arc<Mutex<ActionSet>>) {
        self.event_map.add_order(event_name, act_set)
    }

    pub fn call(&mut self, event_name: &str, context: &Py<PyDict>) {
        self.event_map.call_order(event_name, context)
    }
}

pub struct OrderMap {
    map: HashMap<String, Arc<Mutex<ActionSet>>>
}

impl OrderMap {
    pub fn new() -> Self {
        Self{map: HashMap::new()}
    }

    pub fn add_order(&mut self, order_name: &str, act_set: Arc<Mutex<ActionSet>>) {
        let action_entry = self.map.entry(order_name.to_string()).or_insert(ActionSet::create());
        *action_entry = act_set;
    }

    pub fn call_order(&mut self, act_name: &str, context: &Py<PyDict>) {
        if let Some(action_set) = self.map.get_mut(act_name) {
            let gil = Python::acquire_gil();
            let python = gil.python();

            action_set.lock().unwrap().call_all(python, context.as_ref(python));
        }
    }
}

#[async_trait(?Send)]
pub trait Signal {
    fn add(&mut self, sig_arg: serde_yaml::Value, skill_name: &str, pkg_name: &str, act_set: Arc<Mutex<ActionSet>>) -> Result<()>;
    fn end_load(&mut self, curr_lang: &LanguageIdentifier) -> Result<()>;
    async fn event_loop(&mut self, signal_event: SignalEventShared, config: &Config, base_context: &Py<PyDict>, curr_lang: &LanguageIdentifier) -> Result<()>;
}

#[derive(Clone)]
pub struct SignalRegistry {
    event: SignalEventShared,
    signals: HashMap<String, Rc<RefCell<dyn Signal>>>
}

impl SignalRegistry {

    pub fn new() -> Self {
        let mut signals: HashMap<String, Rc<RefCell<dyn Signal>>> = HashMap::new();
        signals.insert("order".to_string(), Rc::new(RefCell::new(new_signal_order())));

        Self {
            event: Arc::new(Mutex::new(SignalEvent::new())),
            signals
        }
    }

    pub fn extend_and_init_classes_py(&mut self, py: Python, pkg_path: &Path, signal_classes: Vec<(PyObject,PyObject)>) -> Result<HashMap<String, Rc<RefCell<dyn Signal>>>, HalfBakedError> {
        let pkg_path = Arc::new(pkg_path.to_owned());

        let process_list = || -> Result<_> {
            let mut sig_to_add = vec![];

            for (key, val) in  &signal_classes {
                let name = key.to_string();
                // We'll get old items, let's ignore them
                if !self.signals.contains_key(&name) {
                    let pyobj = val.call(py, PyTuple::empty(py), None).map_err(|py_err|
                        anyhow!("Python error while instancing action \"{}\": {:?}", name, py_err.to_string())
                    )?;
                    let sigobj: Rc<RefCell<dyn Signal>> = Rc::new(RefCell::new(PythonSignal::new(key.clone(), pyobj, pkg_path.clone())));
                    sig_to_add.push((name, sigobj));
                }
            }
            Ok(sig_to_add)
        };

        match process_list()  {
            Ok(sig_to_add) => {
                let mut res = HashMap::new();
                for (name, sigobj) in sig_to_add {
                    res.insert(name.clone(), sigobj.clone());
                    self.signals.insert(name, sigobj);
                    
                }
                Ok(res)
            }
            Err(e) => {
                // Process the rest of the list
                Err(HalfBakedError::from(
                    HalfBakedError::gen_diff(&self.signals, signal_classes),
                    e
                ))
            }
        }

    }

    pub fn end_load(&mut self, curr_lang: &LanguageIdentifier) -> Result<()> {

        let mut to_remove = Vec::new();

        for (sig_name, signal) in self.signals.iter_mut() {
            if let Err(e) = signal.borrow_mut().end_load(curr_lang) {
                warn!("Signal \"{}\" had trouble in \"end_load\", will be disabled, error: {}", &sig_name, e);

                to_remove.push(sig_name.to_owned());
            }
        }

        // Delete any signals which had problems during end_load
        for sig_name in &to_remove {
            self.signals.remove(sig_name);
        }

        Ok(())
    }

    pub async fn call_loop(&mut self,
        sig_name: &str,
        config: &Config,
        base_context: &Py<PyDict>,
        curr_lang: &LanguageIdentifier
    ) -> Result<()> {
        self.signals[sig_name].borrow_mut().event_loop(self.event.clone(), config, base_context, curr_lang).await
    }
}

struct PythonSignal {
    sig_inst: PyObject,
    sig_name: Py<PyAny>,
    lily_pkg_path: Arc<PathBuf>
}

impl PythonSignal {
    fn new(sig_name: Py<PyAny>, sig_inst: PyObject, lily_pkg_path: Arc<PathBuf>) -> Self {
        Self {sig_name, sig_inst, lily_pkg_path}
    }

    fn call_py_method<A:IntoPy<Py<PyTuple>>>(&mut self, py: Python, name: &str, args: A, required: bool) -> Result<()> {
        std::env::set_current_dir(self.lily_pkg_path.as_ref())?;
        match self.sig_inst.getattr(py, name) {
            Ok(meth) => {
                call_for_pkg(
                    self.lily_pkg_path.as_ref(),
                    |_| {
                        meth.call(py, args, None).map_err(
                            |py_err|{
                                py_err.clone_ref(py).print(py);
                                anyhow!("Python error while calling {}: {:?}", name, py_err)
                            }
                        )?;
                        Ok(())
                    }
                )?
            }

            Err(e) => {
                if required {
                    Err(e.into())
                }
                else {
                    Ok(())
                }
            }
        }        
    }
}

#[async_trait(?Send)]
impl Signal for PythonSignal {
    fn add(&mut self, sig_arg: serde_yaml::Value, skill_name: &str, pkg_name: &str, act_set: Arc<Mutex<ActionSet>>) -> Result<()> {
        // Pass act_set to python so that Python signals can somehow call their respective actions
        let gil= Python::acquire_gil();
        let py = gil.python();

        let py_arg = yaml_to_python(py, &sig_arg);
        let actset = PyActionSet::from_arc(act_set);

        self.call_py_method(py, "add_sig_receptor", (py_arg, skill_name, pkg_name, actset), true)
    }
    fn end_load(&mut self, curr_lang: &LanguageIdentifier) -> Result<()> {
        let gil= Python::acquire_gil();
        let py = gil.python();

        self.call_py_method(py, "end_load", (curr_lang.to_string(),), false)
    }
    async fn event_loop(&mut self, _signal_event: SignalEventShared, _config: &Config, base_context: &Py<PyDict>
        , curr_lang: &LanguageIdentifier) -> Result<()> {
        let gil= Python::acquire_gil();
        let py = gil.python();

        self.call_py_method(py, "event_loop", (base_context, curr_lang.to_string()), true)
    }
}

impl Drop for PythonSignal {
    fn drop(&mut self) {
        println!("Python signal dropped!");
        let gil= Python::acquire_gil();
        let py = gil.python();
        remove_from_signals(py, &vec![self.sig_name.clone()]).expect(
            &format!("Failed to remove signal: {}", self.sig_name.to_string())
        );
    }
}

// To show each package just those signals available to them
#[derive(Clone)]
pub struct LocalSignalRegistry {
    event: SignalEventShared,
    signals: HashMap<String, Rc<RefCell<dyn Signal>>>,
    global_reg: SignalRegistryShared
}

impl LocalSignalRegistry {
    pub fn init_from(global_reg: SignalRegistryShared) -> Self {
        Self {
            event: {global_reg.borrow().event.clone()},
            signals: {global_reg.borrow().signals.clone()},
            global_reg: {global_reg.clone()}
        }
    }

    pub fn add_sigact_rel(&mut self,sig_name: &str, sig_arg: serde_yaml::Value, skill_name: &str, pkg_name: &str, act_set: Arc<Mutex<ActionSet>>) -> Result<()> {
        if sig_name == "event" {
            self.event.lock().sendable()?.add(skill_name, act_set);
            Ok(())
        }
        else {
            match self.signals.get(sig_name) {
                Some(signal) => signal.borrow_mut().add(sig_arg, skill_name, pkg_name, act_set),
                None => Err(anyhow!("Signal named \"{}\" was not found", sig_name))
            }
        }
    }

    pub fn extend_and_init_classes_py(&mut self, py: Python, pkg_path: &Path, signal_classes: Vec<(PyObject, PyObject)>) -> Result<(), HalfBakedError> {
        self.signals.extend( (*self.global_reg).borrow_mut().extend_and_init_classes_py(py, pkg_path, signal_classes)?);
        Ok(())
    }

    pub fn minus(&self, other: &Self) -> Self {
        let mut res = LocalSignalRegistry{
            event: self.event.clone(),
            signals: HashMap::new(),
            global_reg: self.global_reg.clone()
        };

        for (k,v) in &self.signals {
            if !other.signals.contains_key(k) {
                res.signals.insert(k.clone(), v.clone());
            }
        }

        res
    }

    pub fn remove_from_global(&self) {
        for (sgnl,_) in &self.signals {
            self.global_reg.borrow_mut().signals.remove(sgnl);
        }
    }
}