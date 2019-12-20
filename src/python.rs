use crate::TTS;
use std::path::Path;
use unic_langid::LanguageIdentifier;
use fluent_langneg::negotiate::{negotiate_languages, NegotiationStrategy};
use cpython::{Python, PyList, PyDict, PyString, PythonObject, PyResult, ToPyObject, py_module_initializer, py_fn, py_method_def, FromPyObject};
use yaml_rust::Yaml;
use crate::audio::PlayDevice;
use ref_thread_local::RefThreadLocal;

pub fn yaml_to_python(yaml: &yaml_rust::Yaml, py: Python) -> cpython::PyObject {
    match yaml {
        Yaml::Real(string) => {
            string.parse::<f64>().unwrap().into_py_object(py).into_object()
        }
        Yaml::Integer(int) => {
            int.into_py_object(py).into_object()

        }
        Yaml::Boolean(boolean) => {
            if *boolean {
                cpython::Python::True(py).into_object()
            }
            else {
                cpython::Python::False(py).into_object()
            }
        }
        Yaml::String(string) => {
            string.into_py_object(py).into_object()
        }
        Yaml::Array(array) => {
            let vec: Vec<_> = array.iter().map(|data| yaml_to_python(data, py)).collect();
            cpython::PyList::new(py, &vec).into_object()

        }
        Yaml::Hash(hash) => {
            let dict = PyDict::new(py);
            for (key, value) in hash.iter() {
                dict.set_item(py, yaml_to_python(key,py), yaml_to_python(value, py)).unwrap();
            }
            
            dict.into_object()

        }
        Yaml::Null => {
            cpython::Python::None(py)
        }
        Yaml::BadValue => {
            panic!("Received a BadValue");
        }
        Yaml::Alias(index) => { // Alias are not supported right now, they are insecure and problematic anyway
            format!("Alias, index: {}", index).into_py_object(py).into_object()
        }
    }
}
pub fn python_init() {
    // Add this executable as a Python module
    let mod_name = std::ffi::CString::new("_lily_impl").unwrap();
    unsafe {assert!(python3_sys::PyImport_AppendInittab(mod_name.into_raw(), Some(PyInit__lily_impl)) != -1);};
}

pub fn try_translate(input: &str) -> String {
    if input.chars().nth(0).unwrap() == '$' {
        // Get GIL
        let gil = Python::acquire_gil();
        let python = gil.python();

        let lily_ext = python.import("lily_ext").unwrap();

        // Remove initial $ from translation
        let call_res = lily_ext.call(python, "translate", (&input[1..], PyDict::new(python)), None).unwrap();
        let tuple: PyList = FromPyObject::extract(python, &call_res).unwrap();
        let res = tuple.get_item(python, 0).to_string();
        println!("Translation:{:?}", res);
        res
    }
    else {
        input.to_string()
    }
}

// Define executable module
py_module_initializer!(_lily_impl, init__lily_impl, PyInit__lily_impl, |py, m| {
    m.add(py, "__doc__", "Internal implementations of Lily's Python functions")?;
    m.add(py, "_say", py_fn!(py, python_say(input: &str)))?;
    m.add(py, "__negotiate_lang", py_fn!(py, negotiate_lang(input: &str, available: Vec<String>)))?;

    Ok(())
});

fn negotiate_lang(python: Python, input: &str, available: Vec<String>) -> PyResult<cpython::PyString> {
    let in_lang: LanguageIdentifier = input.parse().unwrap();
    let available_langs: Vec<LanguageIdentifier> = available.iter().map(|lang_str|{lang_str.parse().unwrap()}).collect();
    Ok(negotiate_languages(&[in_lang],&available_langs, None, NegotiationStrategy::Filtering)[0].to_string().into_py_object(python))
}

fn python_say(python: Python, input: &str) -> PyResult<cpython::PyObject> {
    let audio = TTS.borrow_mut().synth_text(input).unwrap();
    PlayDevice::new().unwrap().play(&*audio.buffer, audio.samples_per_second);
    Ok(python.None())
}

pub fn add_to_sys_path(py: Python, path: &Path) -> PyResult<()> {
    let sys = py.import("sys")?;
    let sys_path = sys.get(py, "path")?.cast_into::<PyList>(py)?;
    sys_path.insert_item(py, 1, PyString::new(py, path.to_str().unwrap()).into_object());

    Ok(())
}