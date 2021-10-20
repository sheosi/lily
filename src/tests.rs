#[cfg(feature = "python_skills")]
use crate::python::python_init;

#[cfg(feature = "python_skills")]
use pyo3::Python;

#[cfg(feature = "python_skills")]
#[test]
fn import_python_impl() {
    python_init().unwrap();

    let gil = Python::acquire_gil();
    let python = gil.python();

    let res = python.import("_lily_impl");
    println!("{:?}", res);
    assert!(res.is_ok());

}