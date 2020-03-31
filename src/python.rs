use std::rc::Rc;
use std::path::Path;

use crate::TTS;
use crate::audio::PlayDevice;

use unic_langid::LanguageIdentifier;
use fluent_langneg::negotiate::{negotiate_languages, NegotiationStrategy};
use cpython::{PyClone, Python, PyList, PyDict, PyString, PythonObject, PyResult, ToPyObject, py_module_initializer, py_fn, FromPyObject};
use serde_yaml::Value;
use ref_thread_local::{ref_thread_local, RefThreadLocal};
use anyhow::{anyhow, Result};

ref_thread_local! {
    pub static managed PYTHON_LILY_PKG_NONE: Rc<String> = Rc::new("<None>".to_string());
    pub static managed PYTHON_LILY_PKG_CURR: Rc<String> = PYTHON_LILY_PKG_NONE.borrow().clone();
}

pub fn yaml_to_python(yaml: &serde_yaml::Value, py: Python) -> cpython::PyObject {
    // If for some reason we can't transform, just panic, but the odds should be really small

    match yaml {
        Value::Number(num) => {
            if let Some(int) = num.as_u64() {
                int.into_py_object(py).into_object()
            }
            else {
                // This should be an f64 for sure, so we are sure about the unwrap
                // Note: Inf and NaN cases haven't been tested
                num.as_f64().unwrap().into_py_object(py).into_object()    
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
            let vec: Vec<_> = seq.iter().map(|data| yaml_to_python(data, py)).collect();
            cpython::PyList::new(py, &vec).into_object()

        }
        Value::Mapping(mapping) => {
            let dict = PyDict::new(py);
            for (key, value) in mapping.iter() { // There shouldn't be a problem with this either
                dict.set_item(py, yaml_to_python(key,py), yaml_to_python(value, py)).unwrap();
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

// Define executable module
py_module_initializer!(_lily_impl, init__lily_impl, PyInit__lily_impl, |py, m| {
    m.add(py, "__doc__", "Internal implementations of Lily's Python functions")?;
    m.add(py, "_say", py_fn!(py, python_say(input: &str)))?;
    m.add(py, "__negotiate_lang", py_fn!(py, negotiate_lang(input: &str, available: Vec<String>)))?;
    m.add(py, "log_error", py_fn!(py, log_error(input: &str)))?;
    m.add(py, "log_warn", py_fn!(py, log_warn(input: &str)))?;
    m.add(py, "log_info", py_fn!(py, log_info(input: &str)))?;
    m.add(py, "__get_curr_lily_package", py_fn!(py, get_current_package()))?;
    m.add(py, "_PlayFile__play_file", py_fn!(py, play_file(input: &str)))?;

    Ok(())
});

fn get_current_package(py: Python) -> PyResult<cpython::PyString> {
    let curr_pkg_string = &*PYTHON_LILY_PKG_CURR.borrow().clone();
    Ok(curr_pkg_string.clone().into_py_object(py))
}

fn make_err<T: std::fmt::Debug>(py: Python, err: T) -> cpython::PyErr {
    cpython::PyErr::new::<PyString, String>(py, format!("{:?}", err))
}

fn negotiate_lang(py: Python, input: &str, available: Vec<String>) -> PyResult<cpython::PyString> {
    let in_lang: LanguageIdentifier = input.parse().map_err(|err|make_err(py, err))?;

    // This is done with a for to have control over the return, so that an exception is thrown if
    // an input language string is wrong
    let mut available_langs: Vec<LanguageIdentifier> = Vec::with_capacity(available.len());
    for lang_str in available.iter() {
         available_langs.push(lang_str.parse().map_err(|err|make_err(py, err))?);
    }
    
    Ok(negotiate_languages(&[in_lang],&available_langs, None, NegotiationStrategy::Filtering)[0].to_string().into_py_object(py))
}

fn python_say(python: Python, input: &str) -> PyResult<cpython::PyObject> {
    let audio = TTS.borrow_mut().synth_text(input).map_err(|err|make_err(python, err))?;
    PlayDevice::new().ok_or_else(||make_err(python, "Couldn't obtain play stream"))?.play_audio(audio);
    Ok(python.None())
}

fn log_info(python: Python, input: &str) -> PyResult<cpython::PyObject> {
    log::info!("{}", input);

    Ok(python.None())
}

fn log_warn(python: Python, input: &str) -> PyResult<cpython::PyObject> {
    log::warn!("{}", input);

    Ok(python.None())
}

fn log_error(python: Python, input: &str) -> PyResult<cpython::PyObject>  {
    log::error!("{}", input);

    Ok(python.None())
}

fn play_file(py: Python, input: &str) -> PyResult<cpython::PyObject> {
    PlayDevice::new().unwrap().play_file(input).unwrap();

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