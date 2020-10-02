// Standard library
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

// This crate
use crate::python::{call_for_pkg, yaml_to_python};

// Other crates
use anyhow::{anyhow, Result};
use cpython::{ObjectProtocol, PyClone, PyDict, PyObject, PyTuple, Python};

type ActionRegistryShared = Rc<RefCell<ActionRegistry>>;

#[derive(Debug)]
pub struct ActionRegistry {
    map: HashMap<String, cpython::PyObject>
}

impl ActionRegistry {

    pub fn new() -> Self {
        Self{map: HashMap::new()}
    }

    // Imports all modules from that module and return the new actions
    pub fn extend_and_init_classes(&mut self, python: Python, action_classes: Vec<(PyObject, PyObject)>) -> Result<HashMap<String, cpython::PyObject>> {
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

        let dup_refs = |pair:(&String, &cpython::PyObject)| {
            let (key, val) = pair;
            (key.to_owned(), val.clone_ref(python))
        };

        let new_map: HashMap<String, cpython::PyObject> = self.map.iter().map(dup_refs).collect();
        Self{map: new_map, global_reg: self.global_reg.clone()}
    }
}


#[derive(Debug)]
pub struct LocalActionRegistry {
    map: HashMap<String, cpython::PyObject>,
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

    fn get(&self, action_name: &str) -> Option<&cpython::PyObject> {
        self.map.get(action_name)
    }
}

struct ActionData {
    obj: cpython::PyObject,
    args: cpython::PyObject,
    lily_pkg_path: Rc<PathBuf>
}

pub struct ActionSet {
    acts: Vec<ActionData>
}

impl ActionSet {
    pub fn create() -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {acts: Vec::new()}))
    }
    pub fn add_action(&mut self, py: Python, act_name: &str, yaml: &serde_yaml::Value, action_registry: &LocalActionRegistry, lily_pkg_path: Rc<PathBuf>) -> Result<()>{
        let act_obj = action_registry.get(act_name).ok_or_else(||anyhow!("Action {} is not registered",act_name))?.clone_ref(py);
        self.acts.push(ActionData{obj: act_obj, args: yaml_to_python(py, &yaml), lily_pkg_path});

        Ok(())
    }
    pub fn call_all(&mut self, py: Python, context: &PyDict) -> Result<()> {
        for action in &self.acts {
            let trig_act = action.obj.getattr(py, "trigger_action").map_err(|py_err|anyhow!("Python error while accessing trigger_action: {:?}", py_err))?; 
            std::env::set_current_dir(action.lily_pkg_path.as_ref())?;
            call_for_pkg(action.lily_pkg_path.as_ref(), |_|trig_act.call(py, (action.args.clone_ref(py), context.clone_ref(py)), None).map_err(|py_err|{py_err.clone_ref(py).print(py);anyhow!("Python error while calling action: {:?}", py_err)}))??;
        }

        Ok(())
    }
}