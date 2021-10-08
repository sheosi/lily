use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::actions::{ActionAnswer, ActionContext, ActionInstance};
use crate::exts::LockIt;
use crate::collections::{BaseRegistrySend, GlobalRegSend, LocalBaseRegistrySend};
#[cfg(feature = "python_skills")]
use crate::python::HalfBakedError;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use log::error;
#[cfg(feature = "python_skills")]
use pyo3::{PyObject, Python, types::{PyTuple}};

pub type QueryData = HashMap<String, String>;
pub type QueryResult = Vec<String>;
pub type QueryRegistry = BaseRegistrySend<dyn Query + Send>;
pub type LocalQueryRegistry = LocalBaseRegistrySend<dyn Query + Send, QueryRegistry>;

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

    pub fn extend_and_init_classes_local(que_reg: &mut LocalQueryRegistry,
        py:Python,
        skill_name: String,
        query_classes: Vec<(PyObject, PyObject)>) -> Result<(), HalfBakedError> {
            let queries = Self::do_extend_and_init_classes(&mut que_reg.get_global_mut(), py, skill_name, query_classes)?;
            que_reg.extend_with_map(queries);
            Ok(())
    }

    pub fn do_extend_and_init_classes(
        que_reg: &mut QueryRegistry,
        python: Python,
        skill_name: String,
        query_classes: Vec<(PyObject, PyObject)>
    ) -> Result<HashMap<String, Arc<Mutex<dyn Query + Send>>>, HalfBakedError> {
        let process_list = || -> Result<_> {
            let mut act_to_add = vec![];
            for (key, val) in  &query_classes {
                let name = key.to_string();
                let pyobj = val.call(python, PyTuple::empty(python), None).map_err(|py_err|anyhow!("Python error while instancing action \"{}\": {:?}", name, py_err.to_string()))?;
                let rc: Arc<Mutex<dyn Query + Send>> = Arc::new(Mutex::new(PythonQuery::new(name.clone(),pyobj)));
                act_to_add.push((name,rc));
            }
            Ok(act_to_add)
        };

        match process_list() {
            Ok(act_to_add) => {
                let mut res = HashMap::new();

                for (name, query) in act_to_add {
                    res.insert(name.clone(), query.clone());
                    if let Err (e) = que_reg.insert(skill_name.clone(),name, query) {
                        error!("{}", e);
                    }
                }

                Ok(res)
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
    pub fn new(q: Arc<Mutex<dyn Query + Send>>, name: String) -> Box<Self> {
        Box::new(Self{q, name})
    }
}

#[async_trait(?Send)]
impl ActionInstance for ActQuery {
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