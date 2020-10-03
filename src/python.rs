use std::cell::{RefCell, Ref};
use std::rc::Rc;
use std::path::Path;

use crate::interfaces::CURR_INTERFACE;
use crate::audio::PlayDevice;
use crate::vars::{PYDICT_SET_ERR_MSG, NO_YAML_FLOAT_MSG};

use anyhow::{anyhow, Result};
use pyo3::{conversion::IntoPy, PyErr, Python, types::{PyBool, PyList, PyDict, PyString}, PyObject, PyResult, prelude::*, wrap_pyfunction, FromPyObject, exceptions::*};
use pyo3::{type_object::PyTypeObject};
use fluent_langneg::negotiate::{negotiate_languages, NegotiationStrategy};
use log::info;
use serde_yaml::Value;
use unic_langid::LanguageIdentifier;

thread_local! {
    pub static PYTHON_LILY_PKG: RefString = RefString::new("<None>");
}

pub struct RefString {
    current_val: RefCell<Rc<String>>,
    default_val: Rc<String>
}

impl RefString {
    pub fn new(def: &str) -> Self {
        let default_val = Rc::new(def.to_owned());
        Self {current_val: RefCell::new(default_val.clone()), default_val}
    }

    pub fn clear(&self) {
        self.current_val.replace(self.default_val.clone());
    }

    pub fn set(&self, val: Rc<String>) {
        self.current_val.replace(val);
    }

    pub fn borrow(&self) -> Ref<String> {
        Ref::map(self.current_val.borrow(), |r|r.as_ref())
    }
}

fn extract_name(path: &Path) -> Result<Rc<String>> {
    let os_str = path.file_name().ok_or_else(||anyhow!("Can't get package path's name"))?;
    let pkg_name_str = os_str.to_str().ok_or_else(||anyhow!("Can't transform package path name to str"))?;
    Ok(Rc::new(pkg_name_str.to_string()))
}

pub fn call_for_pkg<F, R>(path: &Path, f: F) -> Result<R> where F: FnOnce(Rc<String>) -> R {
    let canon_path = path.canonicalize()?;
    let pkg_name = extract_name(&canon_path)?;
    std::env::set_current_dir(&canon_path)?;
    PYTHON_LILY_PKG.with(|c| c.set(pkg_name.clone()));
    let r = f(pkg_name);
    PYTHON_LILY_PKG.with(|c| c.clear());

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

    let mod_name = std::ffi::CString::new("_lily_impl")?.into_raw();
    unsafe {assert!(pyo3::ffi::PyImport_AppendInittab(mod_name, Some(safe_lily_impl)) != -1);};

    Ok(())
}


//Have to repeat implementation because can't be genericiced on FromPyObject because
//it requires a lifetime (because of some implementations) which means we can't drop
//the reference to call_res
pub fn try_translate(input: &str) -> Result<String> {
    if let Some(first_letter) = input.chars().nth(0) {
        if first_letter == '$' {

            // Get GIL
            let gil = Python::acquire_gil();
            let python = gil.python();

            let lily_ext = python.import("lily_ext").map_err(|py_err|anyhow!("Python error while importing lily_ext: {:?}", py_err))?;

            // Remove initial $ from translation
            let call_res_result = lily_ext.call("_translate_impl", (&input[1..], PyDict::new(python)), None);
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

pub fn try_translate_all(input: &str) -> Result<Vec<String>> {
    if let Some(first_letter) = input.chars().nth(0) {
        if first_letter == '$' {
                // Get GIL
            let gil = Python::acquire_gil();
            let python = gil.python();

            let lily_ext = python.import("lily_ext").map_err(|py_err|anyhow!("Python error while importing lily_ext: {:?}", py_err))?;

            // Remove initial $ from translation
            let call_res_result = lily_ext.call("_translate_all_impl", (&input[1..], PyDict::new(python)), None);
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
pub fn add_py_folder(python: Python, actions_path: &Path) -> Result<(Vec<(PyObject, PyObject)>, Vec<(PyObject, PyObject)>)> {
    call_for_pkg::<_, Result<()>>(actions_path.parent().ok_or_else(||anyhow!("Can't get parent of path, this is an invalid path for python data"))?, |_|{
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

        Ok(())
    })??;

    let signal_classes: Vec<(PyObject, PyObject)> = {
        let sgn_cls_obj = python.import("lily_ext")?.get("_signal_classes")?;
        sgn_cls_obj.extract()?
    };

    let action_classes: Vec<(PyObject, PyObject)> = {
        let act_cls_obj = python.import("lily_ext")?.get("_action_classes")?;
        act_cls_obj.extract()?
    };

    Ok((signal_classes, action_classes))
}

// Define executable module
#[pymodule]
fn _lily_impl(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add("__doc__", "Internal implementations of Lily's Python functions")?;
    m.add("_say", wrap_pyfunction!(python_say, m)?)?;
    m.add("_negotiate_lang", wrap_pyfunction!(negotiate_lang, m)?)?;
    m.add("log_error", wrap_pyfunction!(log_error, m)?)?;
    m.add("log_warn", wrap_pyfunction!(log_warn, m)?)?;
    m.add("log_info", wrap_pyfunction!(log_info, m)?)?;
    m.add("_get_curr_lily_package", wrap_pyfunction!(get_current_package, m)?)?;
    m.add("_play_file", wrap_pyfunction!(play_file, m)?)?;
    m.add("conf", wrap_pyfunction!(get_conf_string, m)?)?;

    Ok(())
}

#[pyfunction]
fn python_say(py: Python, text: &str) -> PyResult<PyObject> {
    let res = CURR_INTERFACE.with(
        |itf|itf.borrow().lock().py_excep::<PyAttributeError>().and_then(
            |mut i|i.answer(text).py_excep::<PyAttributeError>()
        )
    );

    match res {
        Ok(()) => Ok(py.None()),
        Err(err) => Err(PyErr::new::<PyOSError,_>(format!("Error while playing audio: {:?}", err)))
    }
}

#[pyfunction]
fn get_current_package( ) -> PyResult<String> {
    PYTHON_LILY_PKG.with(|n|
        Ok(n.borrow().clone())
    )
}

#[pyfunction]
fn negotiate_lang(input_lang: &str, default: &str, available: Vec<String>) -> PyResult<String> {
    let in_lang: LanguageIdentifier = input_lang.parse().py_excep::<PyAttributeError>()?;
    let def_lang: LanguageIdentifier = default.parse().py_excep::<PyAttributeError>()?;

    // This is done with a for to have control over the return, so that an exception is thrown if
    // an input language string is wrong
    let mut available_langs: Vec<LanguageIdentifier> = Vec::with_capacity(available.len());
    for lang_str in available.iter() {
         available_langs.push(lang_str.parse().py_excep::<PyAttributeError>()?);
    }
    
    Ok(negotiate_languages(&[in_lang],&available_langs, Some(&def_lang), NegotiationStrategy::Filtering)[0].to_string())
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
fn play_file(py: Python, input: &str) -> PyResult<PyObject> {
    let mut play_dev = PlayDevice::new().map_err(|err|PyErr::new::<PyOSError, _>(format!("Couldn't obtain play stream, reason: {:?}", err)))?;
    if let Err(err) = play_dev.play_file(input) {
        Err(PyErr::new::<PyOSError, _>(format!("Couldn't play file \"{}\": {}",input, err)))
    }
    else {
        Ok(py.None())
    }
}

#[pyfunction]
fn get_conf_string(py: Python, conf_name: &str) -> PyResult<PyObject> {
    let curr_conf = crate::config::GLOBAL_CONF.with(|c|c.borrow().clone());
    let conf_data = PYTHON_LILY_PKG.with(|n| {
        curr_conf.get_package_path(&(&n).borrow(), conf_name)
    });
    Ok(match conf_data {
        Some(string) => string.into_py(py),
        None => py.None()
    })
}

#[pyfunction]
fn do_signal(py: Python, uuid: &str) ->  PyResult<PyObject> {
    Ok(py.None())
}

pub fn add_to_sys_path(py: Python, path: &Path) -> Result<()> {
    let sys = py.import("sys").map_err(|py_err|anyhow!("Failed while importing sys package: {:?}", py_err))?;
    let sys_path = {
        let obj = sys.get("path").map_err(|py_err|anyhow!("Error while getting path module from sys: {:?}", py_err))?;
        obj.cast_as::<PyList>().map_err(|py_err|anyhow!("What? Couldn't get path as a List: {:?}", py_err))?
    };
    
    let path_str = path.to_str().ok_or_else(||anyhow!("Couldn't transform given path to add to sys.path into an str"))?;
    sys_path.insert(1, PyString::new(py, path_str))?;

    Ok(())
}

pub fn set_python_locale(py: Python, lang_id: &LanguageIdentifier) -> Result<()> {
    let locale = py.import("locale").map_err(|py_err|anyhow!("Failed while importing locale package: {:?}", py_err))?;
    let lc_all = locale.get("LC_ALL").map_err(|py_err|anyhow!("Failed to get LC_ALL from locale: {:?}", py_err))?;
    let local_str = format!("{}.UTF-8", lang_id.to_string().replacen("-", "_", 1));
    log::info!("Curr locale: {:?}", local_str);
    locale.call("setlocale", (lc_all, local_str), None).map_err(|py_err|anyhow!("Failed the call to setlocale: {:?}", py_err))?;
    Ok(())
}


/**  Utilities ****************************************************************/

// Transforms any error into a python exception
trait PyException<T> {
    fn py_excep<P: PyTypeObject>(self, ) -> PyResult<T>;
}

impl<T, E: std::fmt::Display> PyException<T> for Result<T,E> {
    fn py_excep<P: PyTypeObject>(self) -> PyResult<T> {
        self.map_err(|e| PyErr::new::<P, _>(format!("{}", e)))
    }
}