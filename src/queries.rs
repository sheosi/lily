use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub type QueryData = HashMap<String, String>;
pub type QueryResult = Vec<String>;
pub trait Query {
    fn is_monitorable(&self) -> bool;
    fn execute(&mut self, data: QueryData) -> QueryResult;
}

struct PythonQuery {

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

pub struct QueryRegistry {

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