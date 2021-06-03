use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::actions::{ActionAnswer, ActionContext, ActionInstance};
use crate::collections::{BaseRegistrySend, LocalBaseRegistrySend};
use crate::python::HalfBakedError;
use crate::vars::POISON_MSG;

use anyhow::Result;
use pyo3::{PyObject, Python};

pub type QueryData = HashMap<String, String>;
pub type QueryResult = Vec<String>;
pub type QueryRegistry = BaseRegistrySend<dyn Query + Send>;
pub type LocalQueryRegistry = LocalBaseRegistrySend<dyn Query + Send, QueryRegistry>;

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

    pub fn extend_and_init_classes_local(_act_reg: &mut LocalQueryRegistry,
        _py:Python,
        _query_classes: Vec<(PyObject, PyObject)>) -> Result<(), HalfBakedError> {
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
    pub fn get_dummy() -> Arc<Mutex<dyn Query + Send>> {
        Arc::new(Mutex::new(DummyQuery::new()))
    }
}
#[derive(Debug)]
pub enum Condition {
    Test
}

impl Condition {
    pub fn check(&self, _query: &Arc<Mutex<dyn Query + Send>>) -> bool {
        false
    }
}
pub struct ActQuery {
    q: Arc<Mutex<dyn Query + Send>>,
    name: String
}

impl ActQuery {
    pub fn new(q: Arc<Mutex<dyn Query + Send>>, name: String) -> Box<Self> {
        Box::new(Self{q, name})
    }
}

impl ActionInstance for ActQuery {
    fn call(&self ,_context: &ActionContext) -> Result<ActionAnswer> {
        let data = HashMap::new();
        let a = self.q.lock().expect(POISON_MSG).execute(data).into_iter().fold("".to_string(), 
        |g,s|format!("{} {},", g, s)
        );

        ActionAnswer::send_text(a, true)
    }
    fn get_name(&self) -> String {
        self.name.clone()
    }
}