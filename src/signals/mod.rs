mod order;

pub use self::order::*;

// Standard library
use std::rc::Rc;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// This crate
use crate::actions::ActionSet;
use crate::config::Config;
use crate::python::call_for_pkg;

// Other crates
use anyhow::{anyhow, Result};
use cpython::{ObjectProtocol, PyClone, PyDict, Python, PyObject, PyTuple, ToPyObject};
use unic_langid::LanguageIdentifier;

pub type SignalEventShared = Rc<RefCell<SignalEvent>>;
type SignalRegistryShared = Rc<RefCell<SignalRegistry>>;

// A especial signal to be called by the system whenever something happens
pub struct SignalEvent {
    event_map: OrderMap
}

impl SignalEvent {
    pub fn new() -> Self {
        Self {event_map: OrderMap::new()}
    }

    pub fn add(&mut self, event_name: &str, act_set: Rc<RefCell<ActionSet>>) {
        self.event_map.add_order(event_name, act_set)
    }

    pub fn call(&mut self, event_name: &str, context: &PyDict) -> Result<()> {
        self.event_map.call_order(event_name, context)
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

    pub fn call_order(&mut self, act_name: &str, context: &PyDict) -> Result<()> {
        if let Some(action_set) = self.map.get_mut(act_name) {
            let gil = Python::acquire_gil();
            let python = gil.python();

            action_set.borrow_mut().call_all(python, context)?;
        }

        Ok(())
    }
}

pub trait Signal {
    fn add(&mut self, sig_arg: serde_yaml::Value, skill_name: &str, pkg_name: &str, act_set: Rc<RefCell<ActionSet>>) -> Result<()>;
    fn end_load(&mut self, curr_lang: &LanguageIdentifier) -> Result<()>;
    fn event_loop(&mut self, signal_event: SignalEventShared, config: &Config, base_context: &PyDict, curr_lang: &LanguageIdentifier) -> Result<()>;
}

#[derive(Clone)]
pub struct SignalRegistry {
    event: SignalEventShared,
    signals: HashMap<String, Rc<RefCell<dyn Signal>>>
}

impl SignalRegistry {
    pub fn new() -> Self {
        let mut signals: HashMap<String, Rc<RefCell<dyn Signal>>> = HashMap::new();
        signals.insert("order".to_string(), Rc::new(RefCell::new(SignalOrder::new())));

        Self {
            event: Rc::new(RefCell::new(SignalEvent::new())),
            signals
        }
    }

    pub fn extend_and_init_classes_py(&mut self, py: Python, pkg_path: &Path, signal_classes: Vec<(PyObject,PyObject)>) -> Result<HashMap<String, Rc<RefCell<dyn Signal>>>> {
        let mut res = HashMap::new();
        let pkg_path = Rc::new(pkg_path.to_owned());

        for (key, val) in  &signal_classes {
            let name = key.to_string();
            // We'll get old items, let's ignore them
            if !self.signals.contains_key(&name) {
                let pyobj = val.call(py, PyTuple::empty(py), None).map_err(|py_err|anyhow!("Python error while instancing action \"{}\": {:?}", name, py_err))?;
                let sigobj: Rc<RefCell<dyn Signal>> = Rc::new(RefCell::new(PythonSignal::new(pyobj, pkg_path.clone())));
                res.insert(name.clone(), sigobj.clone());
                self.signals.insert(name, sigobj);
            }
        }

        Ok(res)
    }

    pub fn end_load(&mut self, curr_lang: &LanguageIdentifier) -> Result<()> {

        for (_, signal) in self.signals.iter_mut() {
            signal.borrow_mut().end_load(curr_lang);
        }
        Ok(())
    }

    pub fn call_loop(&mut self,
        sig_name: &str,
        config: &Config,
        base_context: &PyDict,
        curr_lang: &LanguageIdentifier
    ) -> Result<()> {
        self.signals[sig_name].borrow_mut().event_loop(self.event.clone(), config, base_context, curr_lang)
    }
}

struct PythonSignal {
    sig_inst: PyObject,
    lily_pkg_path: Rc<PathBuf>
}

impl PythonSignal {
    fn new(sig_inst: PyObject, lily_pkg_path: Rc<PathBuf>) -> Self {
        Self {sig_inst, lily_pkg_path}
    }

    fn call_py_method<A:ToPyObject<ObjectType=PyTuple>>(&mut self, py: Python, name: &str, args: A) -> Result<()> {
        let meth = self.sig_inst.getattr(py, name).map_err(|py_err|anyhow!("Python error while accessing {}: {:?}", name, py_err))?;
        std::env::set_current_dir(self.lily_pkg_path.as_ref())?;
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
}


impl Signal for PythonSignal {
    fn add(&mut self, sig_arg: serde_yaml::Value, skill_name: &str, pkg_name: &str, act_set: Rc<RefCell<ActionSet>>) -> Result<()> {
        // TODO: sig_arg and others into the call
        let gil= Python::acquire_gil();
        let py = gil.python();

        self.call_py_method(py, "add_sig_receptor", PyTuple::empty(py))
    }
    fn end_load(&mut self, curr_lang: &LanguageIdentifier) -> Result<()> {
        let gil= Python::acquire_gil();
        let py = gil.python();

        self.call_py_method(py, "end_load", (curr_lang.to_string(),))
    }
    fn event_loop(&mut self, signal_event: SignalEventShared, config: &Config, base_context: &PyDict, curr_lang: &LanguageIdentifier) -> Result<()> {
        let gil= Python::acquire_gil();
        let py = gil.python();

        self.call_py_method(py, "event_loop", (base_context, curr_lang.to_string()))
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

    pub fn add_sigact_rel(&mut self,sig_name: &str, sig_arg: serde_yaml::Value, skill_name: &str, pkg_name: &str, act_set: Rc<RefCell<ActionSet>>) -> Result<()> {
        if sig_name == "event" {
            self.event.borrow_mut().add(skill_name, act_set);
            Ok(())
        }
        else {
            match self.signals.get(sig_name) {
                Some(signal) => signal.borrow_mut().add(sig_arg, skill_name, pkg_name, act_set),
                None => Err(anyhow!("Signal named \"{}\" was not found", sig_name))
            }
        }
    }

    pub fn extend_and_init_classes_py(&mut self, py: Python, pkg_path: &Path, signal_classes: Vec<(PyObject, PyObject)>) -> Result<()> {
        self.signals.extend( (*self.global_reg).borrow_mut().extend_and_init_classes_py(py, pkg_path, signal_classes)?);
        Ok(())
    }
}