use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::actions::{Action, ActionAnswer, ActionContext};
use crate::collections::BaseRegistry;
use crate::exts::LockIt;

use anyhow::Result;
use async_trait::async_trait;
use lazy_static::lazy_static;
use log::error;

pub type QueryData = HashMap<String, String>;
pub type QueryResult = Vec<String>;
pub type QueryRegistry = BaseRegistry<dyn Query + Send>;

lazy_static! {
    pub static ref QUERY_REG: Mutex<QueryRegistry> = Mutex::new(QueryRegistry::new());
}
pub trait Query {
    fn is_monitorable(&self) -> bool;
    fn execute(&mut self, data: QueryData) -> Result<QueryResult, ()>;
    fn get_name(&self) -> &str;
}

struct DummyQuery {}

impl Query for DummyQuery {
    fn is_monitorable(&self) -> bool {
        true
    }
    fn execute(&mut self, _data: QueryData) -> Result<QueryResult, ()> {
        Ok(vec![])
    }
    fn get_name(&self) -> &str {
        "dummy"
    }
}

#[derive(Debug)]
pub enum Condition {
    Changed(RefCell<Vec<String>>),
}

impl Condition {
    pub fn check(&mut self, query: &Arc<Mutex<dyn Query + Send>>, data: QueryData) -> bool {
        match self {
            Condition::Changed(c) => {
                let mut q = query.lock_it();
                match q.execute(data) {
                    Ok(v) => {
                        let res = *c.borrow() == v;
                        *c.borrow_mut() = v; // Store new result for later
                        res
                    }
                    Err(_) => {
                        error!("Query '{}' had an error", q.get_name());
                        false
                    }
                }
            }
        }
    }
}
pub struct ActQuery {
    q: Arc<Mutex<dyn Query + Send>>,
    name: String,
}

impl ActQuery {
    pub fn new(q: Arc<Mutex<dyn Query + Send>>, name: String) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self { q, name }))
    }
}

#[async_trait(?Send)]
impl Action for ActQuery {
    async fn call(&mut self, _context: &ActionContext) -> Result<ActionAnswer> {
        let data = HashMap::new();
        let a = match self.q.lock_it().execute(data) {
            Ok(v) => v
                .into_iter()
                .fold("".to_string(), |g, s| format!("{} {:?},", g, s)),
            Err(_) => "Had an error".into(), // TODO: This should be translated
        };

        ActionAnswer::send_text(a, true)
    }
    fn get_name(&self) -> String {
        self.name.clone()
    }
}
