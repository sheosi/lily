use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::collections::{BaseRegistry, LocalBaseRegistry};
use crate::python::HalfBakedError;

use pyo3::{PyObject, Python};

pub type QueryData = HashMap<String, String>;
pub type QueryResult = Vec<String>;
pub type QueryRegistry = BaseRegistry<dyn Query>;
pub type QueryRegistryShared = Rc<RefCell<QueryRegistry>>;
pub type LocalQueryRegistry = LocalBaseRegistry<dyn Query, QueryRegistry>;

pub trait Query {
    fn is_monitorable(&self) -> bool;
    fn execute(&mut self, data: QueryData) -> QueryResult;
}

pub struct PythonQuery {

}

impl Query for PythonQuery {
    fn is_monitorable(&self) -> bool {true}
    fn execute(&mut self, _data: QueryData) -> QueryResult {
        std::todo!();
    }
}

impl PythonQuery {
    pub fn new() -> Self {
        PythonQuery {}
    }

    pub fn extend_and_init_classes_local(act_reg: &mut LocalQueryRegistry,
        py:Python,
        query_classes: Vec<(PyObject, PyObject)>) -> Result<(), HalfBakedError> {
            Ok(())
    }
}

struct DummyQuery {

}

impl Query for DummyQuery {
    fn is_monitorable(&self) -> bool {true}
    fn execute(&mut self, _data: QueryData) -> QueryResult {
        vec![]
    }
}

impl DummyQuery {
    fn new() -> Self {
        Self {}
    }
}

impl QueryRegistry {
    pub fn get_dummy() -> Rc<RefCell<dyn Query>> {
        Rc::new(RefCell::new(DummyQuery::new()))
    }
}

pub enum Condition {
    Test
}

impl Condition {
    pub fn check(&self, _query: &Rc<RefCell<dyn Query>>) -> bool {
        false
    }
}