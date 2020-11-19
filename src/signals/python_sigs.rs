// Standard library
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::actions::{ActionSet ,PyActionSet};
use crate::config::Config;
use crate::python::{call_for_pkg, HalfBakedError, remove_from_signals, yaml_to_python};
use crate::signals::{LocalSignalRegistry, Signal, SignalEventShared, SignalRegistry};

use async_trait::async_trait;
use anyhow::{anyhow, Result};
use pyo3::{conversion::IntoPy, types::{PyDict, PyTuple}, Py, PyAny, Python, PyObject};
use unic_langid::LanguageIdentifier;


pub struct PythonSignal {
    sig_inst: PyObject,
    sig_name: Py<PyAny>,
    lily_pkg_path: Arc<PathBuf>
}

impl PythonSignal {
    pub fn new(sig_name: Py<PyAny>, sig_inst: PyObject, lily_pkg_path: Arc<PathBuf>) -> Self {
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

    pub fn extend_and_init_classes_py(reg: &mut SignalRegistry, py: Python, pkg_path: &Path, signal_classes: Vec<(PyObject,PyObject)>) -> Result<HashMap<String, Rc<RefCell<dyn Signal>>>, HalfBakedError> {
        let pkg_path = Arc::new(pkg_path.to_owned());
    
        let process_list = || -> Result<_> {
            let mut sig_to_add = vec![];
    
            for (key, val) in  &signal_classes {
                let name = key.to_string();
                // We'll get old items, let's ignore them
                if !reg.contains(&name) {
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
                    reg.insert(name, sigobj).map_err(|e|HalfBakedError::from(sig_to_add))?;
                }
                Ok(res)
            }
            Err(e) => {
                // Process the rest of the list
                Err(HalfBakedError::from(
                    HalfBakedError::gen_diff(reg.get_map_ref(), signal_classes),
                    e
                ))
            }
        }
    
    }
    
    pub fn extend_and_init_classes_py_local(
        reg: &mut LocalSignalRegistry,
        py: Python,
        pkg_path: &Path,
        signal_classes: Vec<(PyObject, PyObject)>
    ) -> Result<(), HalfBakedError> {

        let signals = Self::extend_and_init_classes_py(&mut reg.get_global_mut(), py, pkg_path, signal_classes)?;
        reg.extend_with_map(signals);
        Ok(())
    }
}

#[async_trait(?Send)]
impl Signal for PythonSignal {
    fn add(&mut self, sig_arg: serde_yaml::Value, skill_name: &str,
        pkg_name: &str, act_set: Arc<Mutex<ActionSet>>) -> Result<()> {

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
    async fn event_loop(&mut self, _signal_event: SignalEventShared,
        _config: &Config, base_context: &Py<PyDict>,
        curr_lang: &LanguageIdentifier) -> Result<()> {

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