use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub type QueryData = HashMap<String, String>;
pub trait Query {
    
}

struct PythonQuery {

}

struct DummyQuery {

}

impl Query for DummyQuery {
    
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