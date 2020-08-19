// Standard library
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

// This crate
use crate::python::{add_to_sys_path, call_for_pkg, yaml_to_python};

// Other crates
use anyhow::{anyhow, Result};
use cpython::{ObjectProtocol, PyClone, PyDict, PyTuple, Python};
use log::info;

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
    pub fn add_folder(&mut self, python: Python, actions_path: &Path) -> Result<HashMap<String, cpython::PyObject>> {
        call_for_pkg::<_, Result<HashMap<String, cpython::PyObject>>>(actions_path.parent().ok_or_else(||anyhow!("Can't get parent of path, this is an invalid path for python data"))?, |_|{
            // Add folder to sys.path
            add_to_sys_path(python, actions_path).map_err(|py_err|anyhow!("Python error while adding to sys.path: {:?}", py_err))?;
            info!("Add folder: {}", actions_path.to_str().ok_or_else(||anyhow!("Coudln't transform actions_path into string"))?);
            

            // Make order_map from python's modules
            for entry in std::fs::read_dir(actions_path)? {
                let entry = entry?;
                if entry.path().is_dir() {
                    let mod_name = entry.file_name().into_string().map_err(|os_str|anyhow!("Failed to transform module name into Unicode: {:?}", os_str))?.to_string();
                    python.import(&mod_name).map_err(|py_err|anyhow!("Failed to import a package's python module: {:?}, {:?}", actions_path, py_err))?;
                }
            }

            self.init_classes(python)
        })?
    }

    fn init_classes(&mut self, python: Python) -> Result<HashMap<String, cpython::PyObject>> {
        let mut res = HashMap::new();
        let action_classes = {
            let lily_py_mod = python.import("lily_ext").map_err(|py_err|anyhow!("Python error while importing lily_ext: {:?}", py_err))?;
            let act_cls_obj = lily_py_mod.get(python, "action_classes").map_err(|py_err|anyhow!("Python error while obtaining action_classes: {:?}", py_err))?;
            let act_cls_dict = act_cls_obj.cast_into::<PyDict>(python).map_err(|py_err|anyhow!("Python error while casting action_classes into PyDict: {:?}", py_err))?;
            act_cls_dict.items(python)
        };

        for (key, val) in  action_classes {
            let name = key.to_string();
            // We'll get old items, let's ignore them
            if !self.map.contains_key(&name) {
                println!("Initting: {}", &name);
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
    pub fn new(global_reg: ActionRegistryShared, py: Python, actions_path: &Path) -> Result<Self> {
        let mut reg = Self {map: HashMap::new(), global_reg};
        reg.add_folder(py, actions_path)?;
        Ok(reg)
    }

    pub fn add_folder(&mut self, py:Python, new_actions_path: &Path) -> Result<()> {
        self.map.extend( (*self.global_reg).borrow_mut().add_folder(py, new_actions_path)?);
        Ok(())
    }

    pub fn try_add_and_clone(&mut self, py: Python, new_actions_path: &Path) -> Result<Self> {
        let mut new = self.clone();
        if new_actions_path.is_dir() {
            new.add_folder(py, new_actions_path)?;
        }

        Ok(new)
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
        self.acts.push(ActionData{obj: act_obj, args: yaml_to_python(&yaml, py), lily_pkg_path});

        Ok(())
    }
    pub fn call_all(&mut self, py: Python, context: &PyDict) -> Result<()> {
        for action in self.acts.iter() {
            // TODO: if an object doesn't implement trigger_action, don't panic, but decide what to do
            let trig_act = action.obj.getattr(py, "trigger_action").map_err(|py_err|anyhow!("Python error while accessing trigger_action: {:?}", py_err))?; 
            std::env::set_current_dir(action.lily_pkg_path.as_ref())?;
            call_for_pkg(action.lily_pkg_path.as_ref(), |_|trig_act.call(py, (action.args.clone_ref(py), context.clone_ref(py)), None).map_err(|py_err|{py_err.clone_ref(py).print(py);anyhow!("Python error while calling action: {:?}", py_err)}))??;
        }

        Ok(())
    }
}