use std::collections::HashMap;
use std::env;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use crate::actions::{ActionSet, ACT_REG};
use crate::exts::LockIt;
use crate::queries::QUERY_REG;
use crate::skills::{call_for_skill, PYTHON_LILY_SKILL};
use crate::signals::{
    dynamic_nlu::DynamicNluRequest,
    registries::POLL_SIGNAL,
    order::{dynamic_nlu::{DYNAMIC_NLU_CHANNEL, EntityAddValueRequest}, dev_mgmt::CAPS_MANAGER}
};
use crate::vars::{PYDICT_SET_ERR_MSG, PYTHON_VIRTUALENV, NO_ADD_ENTITY_VALUE_MSG, NO_YAML_FLOAT_MSG};

use anyhow::{anyhow, Result};
use pyo3::{conversion::IntoPy, PyErr, Python, types::{PyBool, PyList, PyDict}, PyObject, PyResult, prelude::*, wrap_pyfunction, FromPyObject, exceptions::*};
use pyo3::type_object::PyTypeObject;
use fluent_langneg::negotiate::{negotiate_languages, NegotiationStrategy};
use log::info;
use serde_yaml::Value;
use thiserror::Error;
use unic_langid::LanguageIdentifier;

thread_local! {
    pub static PYTHON_LILY_SKILL: RefString = RefString::new("<None>");
}

pub fn call_for_skill<F, R>(path: &Path, f: F) -> Result<R> where F: FnOnce(Rc<String>) -> R {
    let canon_path = path.canonicalize()?;
    let skill_name = extract_name(&canon_path)?;
    std::env::set_current_dir(&canon_path)?;
    PYTHON_LILY_SKILL.with(|c| c.set(skill_name.clone()));
    let r = f(skill_name);
    PYTHON_LILY_SKILL.with(|c| c.clear());

    Ok(r)
}


pub fn yaml_to_python(py: Python, yaml: &serde_yaml::Value) -> PyObject {
    // If for some reason we can't transform, just panic, but the odds should be really small

    match yaml {
        Value::Number(num) => {
            if let Some(int) = num.as_u64() {
                int.into_py(py)
            }
            else {
                // This should be an f64 for sure, so we are sure about the expect
                // Note: Inf and NaN cases haven't been tested
                num.as_f64().expect(NO_YAML_FLOAT_MSG).into_py(py) 
            }

        }
        Value::Bool(boolean) => {
            PyBool::new(py, *boolean).into()

        }
        Value::String(string) => {
            string.into_py(py)
        }
        Value::Sequence(seq) => {
            let vec: Vec<_> = seq.iter().map(|data| yaml_to_python(py, data)).collect();
            PyList::new(py, &vec).into()

        }
        Value::Mapping(mapping) => {
            let dict = PyDict::new(py);
            for (key, value) in mapping.iter() { 
                // There shouldn't be a problem with this either
                dict.set_item(yaml_to_python(py, key), yaml_to_python(py, value)).expect(PYDICT_SET_ERR_MSG);
            }
            
            dict.into()

        }
        Value::Null => {
            Python::None(py)
        }
    }
}

pub fn python_init() -> Result<()> {
    // Add this executable as a Python module
    extern "C" fn safe_lily_impl() -> *mut pyo3::ffi::PyObject {
        unsafe{PyInit__lily_impl()}
    }

    let py_env = PYTHON_VIRTUALENV.resolve();
    std::fs::create_dir_all(&py_env)?;
    env::set_var("PYTHON_VIRTUALENV", py_env.as_os_str());
    pyo3::prepare_freethreaded_python();

    let mod_name = std::ffi::CString::new("_lily_impl")?.into_raw();
    unsafe {assert!(pyo3::ffi::PyImport_AppendInittab(mod_name, Some(safe_lily_impl)) != -1);};

    //Make sure we have all deps
    fn check_module_installed(pkg: &str, name_in_fs: &str) -> Result<()> {
        if !python_has_module_path(&Path::new(name_in_fs))? && !Command::new("python3")
            .args(&["-m", "pip", "install", pkg]).status()?.success() {
                log::warn!("Could not install mandatory Python module {}", pkg);
        }

        Ok(())
    }
    
    check_module_installed("snips-nlu", "snips_nlu")?;
    check_module_installed("fluent.runtime", "fluent/runtime")?;


    


    Ok(())
}


//Have to repeat implementation because can't be genericiced on FromPyObject because
//it requires a lifetime (because of some implementations) which means we can't drop
//the reference to call_res
pub fn try_translate(input: &str, lang: &str) -> Result<String> {
    if let Some(first_letter) = input.chars().nth(0) {
        if first_letter == '$' {

            // Get GIL
            let gil = Python::acquire_gil();
            let python = gil.python();

            let lily_ext = python.import("lily_ext").map_err(|py_err|anyhow!("Python error while importing lily_ext: {:?}", py_err))?;

            // Remove initial $ from translation
            let call_res_result = lily_ext.getattr("_translate_impl")?.call((&input[1..], PyDict::new(python), lang), None);
            let call_res = call_res_result.map_err(|py_err|{py_err.clone_ref(python).print(python);anyhow!("lily_ext's \"__translate_impl\" failed, most probably you tried to load an inexistent translation, {:?}", py_err)})?;

            let trans_lst: String = FromPyObject::extract(&call_res).map_err(|py_err|anyhow!("_translate_impl() didn't return a string: {:?}", py_err))?;
            
            Ok(trans_lst)
        }
        else {
            Ok(input.to_string())
        }
    }
    else {
            Ok(input.to_string())
    }
}

pub fn try_translate_all(input: &str, lang: &str) -> Result<Vec<String>> {
    if let Some(first_letter) = input.chars().nth(0) {
        if first_letter == '$' {
                // Get GIL
            let gil = Python::acquire_gil();
            let python = gil.python();

            let lily_ext = python.import("lily_ext").map_err(|py_err|anyhow!("Python error while importing lily_ext: {:?}", py_err))?;

            // Remove initial $ from translation
            let call_res_result = lily_ext.getattr("_translate_all_impl")?.call((&input[1..], PyDict::new(python), lang), None);
            let call_res = call_res_result.map_err(|py_err|{py_err.clone_ref(python).print(python);anyhow!("lily_ext's \"__translate_all_impl\" failed, most probably you tried to load an inexistent translation, {:?}", py_err)})?;

            let trans_lst: Vec<String> = FromPyObject::extract(&call_res).map_err(|py_err|anyhow!("_translate_all_impl() didn't return a list: {:?}", py_err))?;
            
            Ok(trans_lst)
        }
        else {
            Ok(vec![input.to_string()])
        }
    }
    else {
            Ok(vec![input.to_string()])
    }
}

/// Imports every module in the passed path then it returns 
/// the unfiltered lists (with both old and new) of signals
/// (the first one) and actions (the second one)
pub fn add_py_folder(python: Python, actions_path: &Path) -> Result<(Vec<(PyObject, PyObject)>, Vec<(PyObject, PyObject)>, Vec<(PyObject, PyObject)>)> {
    call_for_skill::<_, Result<()>>(actions_path.parent().ok_or_else(||anyhow!("Can't get parent of path, this is an invalid path for python data"))?, |_|{
        // Add folder to sys.path
        sys_path::add(python, actions_path).map_err(|py_err|anyhow!("Python error while adding to sys.path: {:?}", py_err))?;
        info!("Add folder: {}", actions_path.to_str().ok_or_else(||anyhow!("Coudln't transform actions_path into string"))?);


        // Make order_map from python's modules
        for entry in std::fs::read_dir(actions_path)? {
            let entry = entry?;
            if entry.path().is_dir() {
                let mod_name = entry.file_name().into_string().map_err(|os_str|anyhow!("Failed to transform module name into Unicode: {:?}", os_str))?.to_string();
                python.import(&mod_name).map_err(|py_err|anyhow!("Failed to import a package's python module: {:?}, {:?}", actions_path, py_err))?;
            }
        }

        Ok(())
    })??;

    let ext_mod = python.import("lily_ext")?;

    let extract_dict = |name: &str| -> Result<Vec<(PyObject,PyObject)>> {
        let sgn_cls_obj = ext_mod.getattr(name)?; // Get objects
        ext_mod.dict().set_item(name, PyDict::new(python))?; // Reset dict
        let sgn_dict = sgn_cls_obj.downcast::<PyDict>().map_err(|e|anyhow!("signal_classes is not a dict: {}",e))?;
        Ok(sgn_dict.items().extract()?)
    };

    let signal_classes= extract_dict("_signal_classes")?;
    let action_classes = extract_dict("_action_classes")?;
    let query_classes = extract_dict("_query_classes")?;

    Ok((signal_classes, action_classes, query_classes))
}

pub fn get_inst_class_name(py: Python, instance: &PyObject) -> Option<String> {
    let type_obj = instance.getattr(py, "__class__").ok();
    let type_name = type_obj.and_then(|p|p.getattr(py, "__name__").ok());
    type_name.and_then(|p|p.extract(py).ok())
}

pub fn python_has_module_path(module_path: &Path) -> Result<bool> {
    let gil = Python::acquire_gil();
    let py = gil.python();

    let sys_path = sys_path::get(py)?;
    let mut found = false;
    for path in sys_path.iter() {
        let path_str: &str = FromPyObject::extract(path)?;
        let path = Path::new(path_str);
        let lang_path = path.join(module_path);
        if lang_path.exists() {
            found = true;
            break;
        }
    }

    Ok(found)
}

// Define executable module
#[pymodule]
fn _lily_impl(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add("__doc__", "Internal implementations of Lily's Python functions")?;
    m.add("_negotiate_lang", wrap_pyfunction!(negotiate_lang, m)?)?;
    m.add("log_error", wrap_pyfunction!(log_error, m)?)?;
    m.add("log_warn", wrap_pyfunction!(log_warn, m)?)?;
    m.add("log_info", wrap_pyfunction!(log_info, m)?)?;
    m.add("conf", wrap_pyfunction!(get_conf, m)?)?;
    m.add("has_cap", wrap_pyfunction!(client_has_cap, m)?)?;
    m.add("add_entity_value", wrap_pyfunction!(add_entity_value, m)?)?;
    m.add("add_task", wrap_pyfunction!(add_task, m)?)?;
    m.add_class::<crate::actions::PyActionSet>()?;
    m.add_class::<crate::actions::ActionAnswer>()?;

    Ok(())
}



#[pyfunction]
fn negotiate_lang(input_lang: Vec<String>, default: &str, available: Vec<String>) -> PyResult<Vec<String>> {
    let in_langs: Vec<LanguageIdentifier> = input_lang.into_iter().map(|i|i.parse()).collect::<Result<Vec<_>,_>>().py_excep::<PyAttributeError>()?;
    let def_lang: LanguageIdentifier = default.parse().py_excep::<PyAttributeError>()?;

    // This is done with a for to have control over the return, so that an exception is thrown if
    // an input language string is wrong
    let mut available_langs: Vec<LanguageIdentifier> = Vec::with_capacity(available.len());
    for lang_str in available.iter() {
         available_langs.push(lang_str.parse().py_excep::<PyAttributeError>()?);
    }
    
    let res = negotiate_languages(&in_langs,&available_langs, Some(&def_lang), NegotiationStrategy::Filtering);
    Ok(res.into_iter().map(|i|i.to_string()).collect())
}

#[pyfunction]
fn log_info(python: Python, text: &str) -> PyResult<PyObject> {
    log::info!("{}", text);

    Ok(python.None())
}

#[pyfunction]
fn log_warn(python: Python, text: &str) -> PyResult<PyObject> {
    log::warn!("{}", text);

    Ok(python.None())
}

#[pyfunction]
fn log_error(python: Python, text: &str) -> PyResult<PyObject>  {
    log::error!("{}", text);

    Ok(python.None())
}

#[pyfunction]
fn get_conf(py: Python, conf_name: &str) -> PyResult<PyObject> {
    let curr_conf = crate::config::GLOBAL_CONF.with(|c|c.borrow().clone());
    let conf_data = PYTHON_LILY_SKILL.with(|n| {
        curr_conf.get_package_path(&(&n).borrow(), conf_name)
    });
    Ok(match conf_data {
        Some(value) => yaml_to_python(py, value),
        None => py.None()
    })
}

#[pyfunction]
fn client_has_cap(client: &str, cap: &str) -> PyResult<bool> {
    CAPS_MANAGER.with(|c| c.borrow().has_cap(client, cap));
    Ok(true)
}

#[pyfunction]
fn add_entity_value(entity_name: String, value: String, langs: Option<Vec<String>>) -> PyResult<()> {
    // Transform all languages into their LanguageIdentifier forms
    let langs = langs.unwrap_or(Vec::new());
    let langs: Result<Vec<_>,_> = langs.into_iter().map(|l|l.parse()).collect();
    let langs = langs.py_excep::<PyValueError>()?;

    // Get channel and ready request
    let mut m = DYNAMIC_NLU_CHANNEL.lock_it();
    let channel = m.as_mut().ok_or_else(||PyErr::new::<PyOSError, _>(NO_ADD_ENTITY_VALUE_MSG))?;
    let request = EntityAddValueRequest{
        skill: get_current_skill()?,
        entity: entity_name,
        value, langs
    };

    // Send request
    channel.blocking_send(DynamicNluRequest::EntityAddValue(request)).py_excep::<PyOSError>()?;
    Ok(())
}

#[pyfunction]
fn add_task(q_name: String, a_name: String) -> PyResult<()> {
    fn assertion<'a>(why: &'static str)->PyErr {
        PyErr::new::<PyAssertionError,_>(why)
    }

    let n = get_current_skill()?;

    let acts = {
        let map =ACT_REG.lock_it();
        let action = map
        .get(&n, &a_name)
        .ok_or_else(||assertion("This skill does not have requested action"))?;

        ActionSet::create(Arc::downgrade(action))
    };

    let q = {
        let map = QUERY_REG.lock_it();
        map.get(&n, &q_name)
        .ok_or_else(||assertion("Skill has no queries with the requested name"))?
        .clone()
    };
    
    POLL_SIGNAL.lock_it().as_ref()
    .ok_or_else(||assertion("Poll signal not available"))?
    .lock_it().add(q,acts).map_err(|_|assertion("Add poll failed"))?;
    
    Ok(())
}

mod sys_path {
    use std::path::Path;

    use anyhow::{anyhow, Result};
    use pyo3::{types::{PyList, PyString}, Python};

    pub fn get<'a>(py: Python::<'a>)-> Result<&'a PyList> {
        let sys = py.import("sys").map_err(|py_err|anyhow!("Failed while importing sys package: {:?}", py_err))?;
        
        let obj = sys.getattr("path").map_err(|py_err|anyhow!("Error while getting path module from sys: {:?}", py_err))?;
        obj.cast_as::<PyList>().map_err(|py_err|anyhow!("What? Couldn't get path as a List: {:?}", py_err))
    }

    pub fn add(py: Python, path: &Path) -> Result<()> {

        let path_str = path.to_str().ok_or_else(||anyhow!("Couldn't transform given path to add to sys.path into an str"))?;
        self::get(py)?.insert(1, PyString::new(py, path_str))?;

        Ok(())
    }
}

pub fn set_python_locale(py: Python, lang_id: &LanguageIdentifier) -> Result<()> {
    let locale = py.import("locale").map_err(|py_err|anyhow!("Failed while importing locale package: {:?}", py_err))?;
    let lc_all = locale.getattr("LC_ALL").map_err(|py_err|anyhow!("Failed to get LC_ALL from locale: {:?}", py_err))?;
    let local_str = format!("{}.UTF-8", lang_id.to_string().replacen("-", "_", 1));
    log::info!("Curr locale: {:?}", local_str);
    locale.getattr("setlocale")?.call((lc_all, local_str), None).map_err(|py_err|anyhow!("Failed the call to setlocale: {:?}", py_err))?;
    Ok(())
}

/**  Utilities ****************************************************************/

// Transforms any error into a python exception
pub trait PyException<T> {
    fn py_excep<P: PyTypeObject>(self, ) -> PyResult<T>;
}

impl<T, E: std::fmt::Display> PyException<T> for Result<T,E> {
    fn py_excep<P: PyTypeObject>(self) -> PyResult<T> {
        self.map_err(|e| PyErr::new::<P, _>(format!("{}", e)))
    }
}

/** Used by other modules, launched after an error while loading classes, 
contains the error and the new classes*/
#[derive(Debug, Error)]
#[error("{source}")]
pub struct HalfBakedError {
    pub cls_names: Vec<Py<PyAny>>,
    pub source: anyhow::Error,
}

impl HalfBakedError {
    pub fn from(cls_names: Vec<Py<PyAny>>, source: anyhow::Error) -> Self{
        Self{cls_names, source}
    }

    pub fn gen_diff<T>(reg: &HashMap<String, T>, clss: Vec<(PyObject,PyObject)>) -> Vec<Py<PyAny>> {
        let mut res = vec![];
        for (key, _) in  &clss {
            let name = key.to_string();
            if !reg.contains_key(&name)  {
                res.push(key.to_owned());
            }
        }

        res
    }
}

fn get_current_skill( ) -> PyResult<String> {
    PYTHON_LILY_SKILL.with(|n|
        Ok(n.borrow().clone())
    )
}

fn extract_name(path: &Path) -> Result<Rc<String>> {
    let os_str = path.file_name().ok_or_else(||anyhow!("Can't get skill path's name"))?;
    let skill_name_str = os_str.to_str().ok_or_else(||anyhow!("Can't transform skill path name to str"))?;
    Ok(Rc::new(skill_name_str.to_string()))
}