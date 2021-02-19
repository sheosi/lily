use crate::python::python_init;
use pyo3::Python;

#[test]
fn import_python_impl() {
    python_init().unwrap();

    let gil = Python::acquire_gil();
    let python = gil.python();

    let res = python.import("_lily_impl");
    assert!(res.is_ok());

}