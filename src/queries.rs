use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::actions::{Action, ActionAnswer, ActionContext};
use crate::exts::LockIt;
use crate::collections::BaseRegistry;
#[cfg(feature = "python_skills")]
use crate::python::HalfBakedError;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use lazy_static::lazy_static;
use log::error;
#[cfg(feature = "python_skills")]
use pyo3::{PyObject, Python, types::{PyTuple}};

pub type QueryData = HashMap<String, String>;
pub type QueryResult = Vec<String>;
pub type QueryRegistry = BaseRegistry<dyn Query + Send>;

lazy_static! {
    pub static ref QUERY_REG: Mutex<QueryRegistry> = Mutex::new(QueryRegistry::new());
}
pub trait Query {
    fn is_monitorable(&self) -> bool;
    fn execute(&mut self, data: QueryData) -> Result<QueryResult,()>;
    fn get_name(&self) -> &str;
}

#[cfg(feature = "python_skills")]
pub struct PythonQuery {
    name: String,
    py_obj: PyObject,
}

#[cfg(feature = "python_skills")]
impl Query for PythonQuery {
    fn is_monitorable(&self) -> bool {true}
    fn execute(&mut self, _data: QueryData) -> Result<QueryResult,()> {
        let gil = Python::acquire_gil();
        let py = gil.python();
        self.py_obj
        .call_method0(py,"get")
        .map_err(|_|())
        .and_then(|r|r.extract::<String>(py).map_err(|_|()))
        .map(|s|vec![s])
    }

    fn get_name(&self) -> &str {
        &self.name
    }
}

#[cfg(feature = "python_skills")]
impl PythonQuery {
    pub fn new(name: String, py_obj: PyObject) -> Self {
        PythonQuery {name, py_obj}
    }

    pub fn extend_and_init_classes(
        python: Python,
        skill_name: String,
        query_classes: Vec<(PyObject, PyObject)>
    ) -> Result<Vec<String>, HalfBakedError> {
        let mut que_reg = QUERY_REG.lock_it();
        let process_list = || -> Result<_> {
            let mut quer_to_add = vec![];
            for (key, val) in  &query_classes {
                let name = key.to_string();
                let pyobj = val.call(python, PyTuple::empty(python), None).map_err(|py_err|anyhow!("Python error while instancing action \"{}\": {:?}", name, py_err.to_string()))?;
                let rc: Arc<Mutex<dyn Query + Send>> = Arc::new(Mutex::new(PythonQuery::new(name.clone(),pyobj)));
                quer_to_add.push((name,rc));
            }
            Ok(quer_to_add)
        };

        match process_list() {
            Ok(quer_to_add) => {
                let queries = quer_to_add.iter().map(|(n, _)| n.clone()).collect();
                for (name, query) in quer_to_add {
                    if let Err (e) = que_reg.insert(&skill_name,&name, query) {
                        error!("{}", e);
                    }
                }

                Ok(queries)
            }

            Err(e) => {
                Err(HalfBakedError::from(
                    HalfBakedError::gen_diff(&que_reg.get_map_ref(), query_classes),
                    e
                ))
            }
        }
    } 
}

struct DummyQuery {

}

impl Query for DummyQuery {
    fn is_monitorable(&self) -> bool {true}
    fn execute(&mut self, _data: QueryData) -> Result<QueryResult,()> {
        Ok(vec![])
    }
    fn get_name(&self) -> &str {
        "dummy"
    }
}

#[derive(Debug)]
pub enum Condition {
    Changed(RefCell<Vec<String>>)
}

impl Condition {
    pub fn check(&mut self, query: &Arc<Mutex<dyn Query + Send>>, data: QueryData) -> bool {
        match self {
            Condition::Changed(c)=>{
                let mut q =query.lock_it();
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
    name: String
}

impl ActQuery {
    pub fn new(q: Arc<Mutex<dyn Query + Send>>, name: String) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self{q, name}))
    }
}

#[async_trait(?Send)]
impl Action for ActQuery {
    async fn call(&self ,_context: &ActionContext) -> Result<ActionAnswer> {
        let data = HashMap::new();
        let a = match self.q.lock_it().execute(data) {
            Ok(v)=>v.into_iter().fold("".to_string(),|g,s|format!("{} {:?},", g, s)),
            Err(_)=> "Had an error".into() // TODO: This should be translated
        };
        

        ActionAnswer::send_text(a, true)
    }
    fn get_name(&self) -> String {
        self.name.clone()
    }
}