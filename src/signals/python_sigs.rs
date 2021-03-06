// Standard library
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::actions::{ActionContext, ActionSet, PyActionSet};
use crate::collections::GlobalRegSend;
use crate::config::Config;
use crate::skills::call_for_skill;
use crate::python::HalfBakedError;
use crate::signals::{LocalSignalRegistry, Signal, SignalEventShared, SignalRegistry, UserSignal};

use async_trait::async_trait;
use anyhow::{anyhow, Result};
use pyo3::{conversion::IntoPy, types::PyTuple, Py, Python, PyObject};
use unic_langid::LanguageIdentifier;


pub struct PythonSignal {
    sig_inst: PyObject,
    sig_name: String,
    lily_skill_path: Arc<PathBuf>
}

impl PythonSignal {
    pub fn new(sig_name: String, sig_inst: PyObject, lily_skill_path: Arc<PathBuf>) -> Self {
        Self {sig_name, sig_inst, lily_skill_path}
    }

    fn call_py_method<A:IntoPy<Py<PyTuple>>>(&mut self, py: Python, name: &str, args: A, required: bool) -> Result<()> {
        std::env::set_current_dir(self.lily_skill_path.as_ref())?;
        match self.sig_inst.getattr(py, name) {
            Ok(meth) => {
                call_for_skill(
                    self.lily_skill_path.as_ref(),
                    |_| {
                        meth.call(py, args, None).map_err(
                            |py_err|{
                                py_err.clone_ref(py).print(py);
                                anyhow!("Python error in signal '{}' while calling {}: {:?}", &self.sig_name, name, py_err)
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

    pub fn extend_and_init_classes_py(reg: &mut SignalRegistry, py: Python, skill_name: String, skill_path: &Path, signal_classes: Vec<(PyObject,PyObject)>) -> Result<HashMap<String, Arc<Mutex<dyn UserSignal + Send>>>, HalfBakedError> {
        let skill_path = Arc::new(skill_path.to_owned());
    
        let process_list = || -> Result<_> {
            let mut sig_to_add = vec![];
    
            for (key, val) in  &signal_classes {
                let name = key.to_string();
                let pyobj = val.call(py, PyTuple::empty(py), None).map_err(|py_err|
                    anyhow!("Python error while instancing signal \"{}\": {:?}", name, py_err.to_string())
                )?;
                let sigobj: Arc<Mutex<dyn UserSignal + Send>> = Arc::new(Mutex::new(PythonSignal::new(name.clone(), pyobj, skill_path.clone())));
                sig_to_add.push((name, sigobj));
            }
            Ok(sig_to_add)
        };
    
        match process_list()  {
            Ok(sig_to_add) => {
                let mut res = HashMap::new();
                let process = || -> Result<()> {
                    for (name, sigobj) in sig_to_add {
                        res.insert(name.clone(), sigobj.clone());
                        reg.insert(skill_name.clone(), name, sigobj)?;
                    }
                    Ok(())
                };
                process().map_err(|e|HalfBakedError::from(
                    HalfBakedError::gen_diff(reg.get_map_ref(), signal_classes),
                    e
                ))?;
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
        skill_name: String,
        skill_path: &Path,
        signal_classes: Vec<(PyObject, PyObject)>
    ) -> Result<(), HalfBakedError> {

        let signals = Self::extend_and_init_classes_py(&mut reg.get_global_mut(), py, skill_name, skill_path, signal_classes)?;
        reg.extend_with_map(signals);
        Ok(())
    }
}

#[async_trait(?Send)]
impl Signal for PythonSignal {
    fn end_load(&mut self, curr_langs: &Vec<LanguageIdentifier>) -> Result<()> {
        let gil= Python::acquire_gil();
        let py = gil.python();

        let curr_langs: Vec<String> = curr_langs.into_iter().map(|i|i.to_string()).collect();
        self.call_py_method(py, "end_load", (curr_langs,), false)
    }
    async fn event_loop(&mut self, _signal_event: SignalEventShared,
        _config: &Config, base_context: &ActionContext,
        curr_langs: &Vec<LanguageIdentifier>) -> Result<()> {

        let gil= Python::acquire_gil();
        let py = gil.python();

        let curr_langs: Vec<String> = curr_langs.into_iter().map(|i|i.to_string()).collect();
        self.call_py_method(py, "event_loop", (base_context.to_owned(), curr_langs), true)
    }
}

impl UserSignal for PythonSignal {
    fn add(&mut self, data: HashMap<String, String>,
        skill_name: &str, act_set: Arc<Mutex<ActionSet>>) -> Result<()> {

        // Pass act_set to python so that Python signals can somehow call their respective actions
        let gil= Python::acquire_gil();
        let py = gil.python();

        let actset = PyActionSet::from_arc(act_set);
        self.call_py_method(py, "add_sig_receptor", (data, skill_name, actset), true)
    }
}