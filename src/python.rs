use std::cell::{RefCell, Ref};
use std::rc::Rc;
use std::path::Path;

use crate::interfaces::CURR_INTERFACE;
use crate::audio::PlayDevice;
use crate::vars::{PYDICT_SET_ERR_MSG, NO_YAML_FLOAT_MSG};

use anyhow::{anyhow, Result};
use cpython::{PyErr, PyClone, Python, PyList, PyDict, PyString, PyObject, PythonObject, PyResult, ToPyObject, py_module_initializer, py_fn, FromPyObject, exc};
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


pub fn yaml_to_python(py: Python, yaml: &serde_yaml::Value) -> cpython::PyObject {
    // If for some reason we can't transform, just panic, but the odds should be really small

    match yaml {
        Value::Number(num) => {
            if let Some(int) = num.as_u64() {
                int.into_py_object(py).into_object()
            }
            else {
                // This should be an f64 for sure, so we are sure about the expect
                // Note: Inf and NaN cases haven't been tested
                num.as_f64().expect(NO_YAML_FLOAT_MSG).into_py_object(py).into_object()    
            }

        }
        Value::Bool(boolean) => {
            if *boolean {
                cpython::Python::True(py).into_object()
            }
            else {
                cpython::Python::False(py).into_object()
            }
        }
        Value::String(string) => {
            string.into_py_object(py).into_object()
        }
        Value::Sequence(seq) => {
            let vec: Vec<_> = seq.iter().map(|data| yaml_to_python(py, data)).collect();
            cpython::PyList::new(py, &vec).into_object()

        }
        Value::Mapping(mapping) => {
            let dict = PyDict::new(py);
            for (key, value) in mapping.iter() { 
                // There shouldn't be a problem with this either
                dict.set_item(py, yaml_to_python(py, key), yaml_to_python(py, value)).expect(PYDICT_SET_ERR_MSG);
            }
            
            dict.into_object()

        }
        Value::Null => {
            cpython::Python::None(py)
        }
    }
}
pub fn python_init() -> Result<()> {
    // Add this executable as a Python module
    let mod_name = std::ffi::CString::new("_lily_impl")?;
    unsafe {assert!(python3_sys::PyImport_AppendInittab(mod_name.into_raw(), Some(PyInit__lily_impl)) != -1);};

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
            let call_res_result = lily_ext.call(python, "_translate_impl", (&input[1..], PyDict::new(python)), None);
            let call_res = call_res_result.map_err(|py_err|{py_err.clone_ref(python).print(python);anyhow!("lily_ext's \"__translate_impl\" failed, most probably you tried to load an inexistent translation, {:?}", py_err)})?;

            let trans_lst: String = FromPyObject::extract(python, &call_res).map_err(|py_err|anyhow!("_translate_impl() didn't return a string: {:?}", py_err))?;
            
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
            let call_res_result = lily_ext.call(python, "_translate_all_impl", (&input[1..], PyDict::new(python)), None);
            let call_res = call_res_result.map_err(|py_err|{py_err.clone_ref(python).print(python);anyhow!("lily_ext's \"__translate_all_impl\" failed, most probably you tried to load an inexistent translation, {:?}", py_err)})?;

            let trans_lst: Vec<String> = FromPyObject::extract(python, &call_res).map_err(|py_err|anyhow!("_translate_all_impl() didn't return a list: {:?}", py_err))?;
            
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

    let signal_classes = {
        let lily_py_mod = python.import("lily_ext").map_err(|py_err|anyhow!("Python error while importing lily_ext: {:?}", py_err))?;
        let sgn_cls_obj = lily_py_mod.get(python, "_signal_classes").map_err(|py_err|anyhow!("Python error while obtaining signal_classes: {:?}", py_err))?;
        let sgn_cls_dict = sgn_cls_obj.cast_into::<PyDict>(python).map_err(|py_err|anyhow!("Python error while casting signal_classes into PyDict: {:?}", py_err))?;
        sgn_cls_dict.items(python)
    };

    let action_classes = {
        let lily_py_mod = python.import("lily_ext").map_err(|py_err|anyhow!("Python error while importing lily_ext: {:?}", py_err))?;
        let act_cls_obj = lily_py_mod.get(python, "_action_classes").map_err(|py_err|anyhow!("Python error while obtaining action_classes: {:?}", py_err))?;
        let act_cls_dict = act_cls_obj.cast_into::<PyDict>(python).map_err(|py_err|anyhow!("Python error while casting action_classes into PyDict: {:?}", py_err))?;
        act_cls_dict.items(python)
    };

    Ok((signal_classes, action_classes))
}

// Define executable module
py_module_initializer!(_lily_impl, init__lily_impl, PyInit__lily_impl, |py, m| {
    m.add(py, "__doc__", "Internal implementations of Lily's Python functions")?;
    m.add(py, "_say", py_fn!(py, python_say(text: &str)))?;
    m.add(py, "_negotiate_lang", py_fn!(py, negotiate_lang(input_lang: &str, default: &str, available: Vec<String>)))?;
    m.add(py, "log_error", py_fn!(py, log_error(text: &str)))?;
    m.add(py, "log_warn", py_fn!(py, log_warn(text: &str)))?;
    m.add(py, "log_info", py_fn!(py, log_info(text: &str)))?;
    m.add(py, "_get_curr_lily_package", py_fn!(py, get_current_package()))?;
    m.add(py, "_play_file", py_fn!(py, play_file(file_name: &str)))?;
    m.add(py, "conf", py_fn!(py,get_conf_string(conf_name: &str)))?;

    Ok(())
});

fn get_current_package(py: Python) -> PyResult<cpython::PyString> {
    PYTHON_LILY_PKG.with(|n|
        Ok(n.borrow().clone().into_py_object(py))
    )
}

fn make_err<T: std::fmt::Debug>(py: Python, err: T) -> cpython::PyErr {
    cpython::PyErr::new::<exc::AttributeError, _>(py, format!("{:?}", err))
}

fn negotiate_lang(py: Python, input_lang: &str, default: &str, available: Vec<String>) -> PyResult<cpython::PyString> {
    let in_lang: LanguageIdentifier = input_lang.parse().map_err(|err|make_err(py, err))?;
    let def_lang: LanguageIdentifier = default.parse().map_err(|err|make_err(py, err))?;

    // This is done with a for to have control over the return, so that an exception is thrown if
    // an input language string is wrong
    let mut available_langs: Vec<LanguageIdentifier> = Vec::with_capacity(available.len());
    for lang_str in available.iter() {
         available_langs.push(lang_str.parse().map_err(|err|make_err(py, err))?);
    }
    
    Ok(negotiate_languages(&[in_lang],&available_langs, Some(&def_lang), NegotiationStrategy::Filtering)[0].to_string().into_py_object(py))
}

fn python_say(py: Python, text: &str) -> PyResult<cpython::PyObject> {
    let res = CURR_INTERFACE.with(|itf|itf.borrow().lock().map_err(|e|make_err(py, e)).and_then(|mut i|i.answer(text).map_err(|e|make_err(py, e))));
    match res {
        Ok(()) => Ok(py.None()),
        Err(err) => Err(PyErr::new::<exc::OSError,_>(py, format!("Error while playing audio: {:?}", err)))
    }
}

fn log_info(python: Python, text: &str) -> PyResult<cpython::PyObject> {
    log::info!("{}", text);

    Ok(python.None())
}

fn log_warn(python: Python, text: &str) -> PyResult<cpython::PyObject> {
    log::warn!("{}", text);

    Ok(python.None())
}

fn log_error(python: Python, text: &str) -> PyResult<cpython::PyObject>  {
    log::error!("{}", text);

    Ok(python.None())
}

fn play_file(py: Python, input: &str) -> PyResult<cpython::PyObject> {
    let mut play_dev = PlayDevice::new().map_err(|err|PyErr::new::<exc::OSError, _>(py, format!("Couldn't obtain play stream, reason: {:?}", err)))?;
    if let Err(err) = play_dev.play_file(input) {
        Err(PyErr::new::<exc::OSError, _>(py, format!("Couldn't play file \"{}\": {}",input, err)))
    }
    else {
        Ok(py.None())
    }
}

fn get_conf_string(py: Python, conf_name: &str) -> PyResult<cpython::PyObject> {
    let curr_conf = crate::config::GLOBAL_CONF.with(|c|c.borrow().clone());
    let conf_data = PYTHON_LILY_PKG.with(|n| {
        curr_conf.get_package_path(&(&n).borrow(), conf_name)
    });
    Ok(match conf_data {
        Some(string) => string.to_py_object(py).into_object(),
        None => py.None()
    })
}

fn do_signal(py: Python, uuid: &str) ->  PyResult<cpython::PyObject> {
    Ok(py.None())
}

pub fn add_to_sys_path(py: Python, path: &Path) -> Result<()> {
    let sys = py.import("sys").map_err(|py_err|anyhow!("Failed while importing sys package: {:?}", py_err))?;
    let sys_path = {
        let obj = sys.get(py, "path").map_err(|py_err|anyhow!("Error while getting path module from sys: {:?}", py_err))?;
        obj.cast_into::<PyList>(py).map_err(|py_err|anyhow!("What? Couldn't get path as a List: {:?}", py_err))?
    };
    
    let path_str = path.to_str().ok_or_else(||anyhow!("Couldn't transform given path to add to sys.path into an str"))?;
    sys_path.insert(py, 1, PyString::new(py, path_str).into_object());

    Ok(())
}

pub fn set_python_locale(py: Python, lang_id: &LanguageIdentifier) -> Result<()> {
    let locale = py.import("locale").map_err(|py_err|anyhow!("Failed while importing locale package: {:?}", py_err))?;
    let lc_all = locale.get(py, "LC_ALL").map_err(|py_err|anyhow!("Failed to get LC_ALL from locale: {:?}", py_err))?;
    let local_str = format!("{}.UTF-8", lang_id.to_string().replacen("-", "_", 1));
    log::info!("Curr locale: {:?}", local_str);
    locale.call(py, "setlocale", (lc_all, local_str), None).map_err(|py_err|anyhow!("Failed the call to setlocale: {:?}", py_err))?;
    Ok(())
}
