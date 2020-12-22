// Standard library
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

// Other crates
use pyo3::{Py, PyAny, PyErr, PyIterProtocol, PyRef, PyRefMut, PyResult, Python, exceptions::*, types::{PyDict, PyIterator, PyTuple, PyType}};
use pyo3::prelude::{pyclass, pymethods, pyproto};
use pyo3::class::PyObjectProtocol;
use pyo3::mapping::PyMappingProtocol;
use pyo3::sequence::PySequenceProtocol;


struct BaseIterator {
    inner: Arc<Mutex<HashMap<String,String>>>,
    count: usize,
    reversed: bool
}

impl BaseIterator {
    fn new(inner: Arc<Mutex<HashMap<String,String>>>) -> Self {
        Self {
            inner,
            count: 0,
            reversed: false
        }
    }

    fn reversed(inner: Arc<Mutex<HashMap<String,String>>>) -> Self {
        Self {
            count: inner.lock().unwrap().len(),
            inner: inner.clone(),
            reversed: true
        }
    }

    fn is_end(&self) -> bool {
        if self.reversed{self.count == 0}
        else {self.count == self.inner.lock().unwrap().len()}
    }

    fn count(&mut self) -> usize {
        let old_count = self.count;
        self.count = if self.reversed{self.count -1} else{self.count + 1};
        old_count
    }
    fn get_inner(&self) -> MutexGuard<HashMap<String,String>> {
        self.inner.lock().unwrap()
    }
}


#[pyclass]
pub struct ActionContextItemsIterator {
    base: BaseIterator
}

impl ActionContextItemsIterator {
    fn new(inner: Arc<Mutex<HashMap<String,String>>>)-> Self {
        Self { base: BaseIterator::new(inner)}
    }

    fn reversed(inner: Arc<Mutex<HashMap<String,String>>>)-> Self {
        Self { base: BaseIterator::reversed(inner)}
    }
}

#[pyproto]
impl PyIterProtocol for ActionContextItemsIterator {
    fn __iter__(slf: PyRefMut<Self>) -> Py<ActionContextItemsIterator> {
        slf.into()
    }

    
    fn __next__(mut slf: PyRefMut<Self>) -> Option<(String, String)> {
        if !slf.base.is_end(){
            let count = slf.base.count();
            slf.base.get_inner().iter().nth(count).map(|(a,b)|(a.into(), b.into()))
        }
        else {None}
    }
}

#[pyclass]
struct ActionContextValuesIterator {
    base: BaseIterator
}

impl ActionContextValuesIterator {
    fn new(inner: Arc<Mutex<HashMap<String,String>>>)-> Self {
        Self { base: BaseIterator::new(inner)}
    }

    fn reversed(inner: Arc<Mutex<HashMap<String,String>>>)-> Self {
        Self { base: BaseIterator::reversed(inner)}
    }
}

#[pyproto]
impl PyIterProtocol for ActionContextValuesIterator {
    fn __iter__(slf: PyRefMut<Self>) -> Py<ActionContextValuesIterator> {
        slf.into()
    }

    
    fn __next__(mut slf: PyRefMut<Self>) -> Option<String> {
        if !slf.base.is_end(){
            let count = slf.base.count();
            slf.base.get_inner().values().nth(count).map(|a|a.into())
        }
        else {None}
    }
}

#[pyclass]
struct ActionContextKeysIterator {
    base: BaseIterator
}

impl ActionContextKeysIterator {
    fn new(inner: Arc<Mutex<HashMap<String,String>>>)-> Self {
        Self { base: BaseIterator::new(inner)}
    }

    fn reversed(inner: Arc<Mutex<HashMap<String,String>>>)-> Self {
        Self { base: BaseIterator::reversed(inner)}
    }
}

#[pyproto]
impl PyIterProtocol for ActionContextKeysIterator {
    fn __iter__(slf: PyRefMut<Self>) -> Py<ActionContextKeysIterator> {
        slf.into()
    }

    
    fn __next__(mut slf: PyRefMut<Self>) -> Option<String> {
        if !slf.base.is_end(){
            let count = slf.base.count();
            slf.base.get_inner().keys().nth(count).map(|a|a.into())
        }
        else {None}
    }
}


#[pyclass]
struct ActionContextItemsView {
    inner: Arc<Mutex<HashMap<String, String>>>,
}

impl ActionContextItemsView {
    fn from(ctx: &ActionContext) -> Self {
        Self{inner: ctx.map.clone()}
    }
}


#[pymethods]
impl ActionContextItemsView {
    fn __len__(&self) -> usize {
        self.inner.lock().unwrap().len()
    }
    
    fn __iter__(slf: PyRef<Self>) -> PyResult<Py<ActionContextItemsIterator>> {
        Py::new(slf.py(),ActionContextItemsIterator::new(slf.inner.clone()))
    }

    fn __reversed__(slf: PyRef<Self>) -> PyResult<Py<ActionContextItemsIterator>> {
        Py::new(slf.py(),ActionContextItemsIterator::reversed(slf.inner.clone()))
    }

}

#[pyclass]
struct ActionContextValuesView {
    inner: Arc<Mutex<HashMap<String, String>>>,
}

impl ActionContextValuesView {
    fn from(ctx: &ActionContext) -> Self {
        Self{inner: ctx.map.clone()}
    }
}


#[pymethods]
impl ActionContextValuesView {
    fn __len__(&self) -> usize {
        self.inner.lock().unwrap().len()
    }
    
    fn __iter__(slf: PyRef<Self>) -> PyResult<Py<ActionContextValuesIterator>> {
        Py::new(slf.py(),ActionContextValuesIterator::new(slf.inner.clone()))
    }

    fn __reversed__(slf: PyRef<Self>) -> PyResult<Py<ActionContextValuesIterator>> {
        Py::new(slf.py(),ActionContextValuesIterator::reversed(slf.inner.clone()))
    }

}

#[pyclass]
struct ActionContextKeysView {
    inner: Arc<Mutex<HashMap<String, String>>>,
}

impl ActionContextKeysView {
    fn from(ctx: &ActionContext) -> Self {
        Self{inner: ctx.map.clone()}
    }
}

#[pymethods]
impl ActionContextKeysView {
    fn __len__(&self) -> usize {
        self.inner.lock().unwrap().len()
    }
    
    fn __iter__(slf: PyRef<Self>) -> PyResult<Py<ActionContextKeysIterator>> {
        Py::new(slf.py(),ActionContextKeysIterator::new(slf.inner.clone()))
    }

    fn __reversed__(slf: PyRef<Self>) -> PyResult<Py<ActionContextKeysIterator>> {
        Py::new(slf.py(),ActionContextKeysIterator::reversed(slf.inner.clone()))
    }

}

#[pyproto]
impl PySequenceProtocol for ActionContextItemsView {
    fn __contains__(&self, k: &str) -> bool {
        self.inner.lock().unwrap().contains_key(k)
    }
}
#[pyclass]
#[derive(Clone)]
pub struct ActionContext {
    map: Arc<Mutex<HashMap<String, String>>>,
}

impl ActionContext {
    pub fn new() -> Self {
        Self{map: Arc::new(Mutex::new(HashMap::new()))}
    }

    pub fn set(&mut self, key: String, value: String) {
        self.map.lock().unwrap().insert(key, value);
    }
}

#[pyproto]
impl PyMappingProtocol for ActionContext {
    fn __delitem__(&mut self, key: &str) -> PyResult<()> {
        self.map.lock().unwrap().remove(key).ok_or(PyErr::new::<PyAttributeError, _>(
            format!("Key: {} was not found in context", key)
        ))?;
        Ok(())
    }

    fn __getitem__(&self, key: &str) -> PyResult<String> {
        self.get(key).ok_or(PyErr::new::<PyAttributeError, _>(
            format!("Key: {} was not found in context", key)
        ))
    }

    fn __len__(&self) -> usize {
        self.map.lock().unwrap().len()
    }

    fn __setitem__(&mut self, key: String, item: String) {
        self.set(key,item);
    }
}

#[pyproto]
impl PyObjectProtocol for ActionContext {
    fn __repr__(&self) -> String {
        // TODO: Maybe improve this one and make it more Pythonic
        format!("{:?}",self.map)
    }

    fn __str__(&self) -> String {
        // TODO: Maybe improve this one and make it more Pythonic
        format!("{:?}",self.map)
    }
}

#[pyproto]
impl PySequenceProtocol for ActionContext {
    fn __contains__(&self, k: &str) -> bool {
        self.map.lock().unwrap().contains_key(k)
    }
}

#[pyproto]
impl PyIterProtocol for  ActionContext {
    fn __iter__(slf: PyRef<Self>) -> PyResult<Py<ActionContextItemsIterator>> {
        Py::new(slf.py(),ActionContextItemsIterator::new(slf.map.clone()))
    }
}

#[pymethods]
impl ActionContext {
    // classmethods
    #[classmethod]
    fn fromkeys(_cls: &&PyType, py:Python, iterable: &PyAny, value: &str) -> PyResult<ActionContext> {
        let mut map = HashMap::new();
        let it = PyIterator::from_object(py,iterable.call_method("__iter__", (), None)?)?;
        for key in it {
            let key: String = key?.extract()?;
            map.insert(key,value.to_owned());
        }

        Ok(Self {map: Arc::new(Mutex::new(map))})
    }

    fn __eq__(&mut self, other: &PyAny) -> bool {
        match other.extract::<ActionContext>() {
            Ok(c) => *self.map.lock().unwrap() == *c.map.lock().unwrap(),
            Err(_) => false
        }
    }

    fn __lt__(&mut self, other: PyRef<ActionContext>) -> bool {
        self.map.lock().unwrap().len() < other.map.lock().unwrap().len()
    }

    fn __reversed__(slf: PyRef<Self>) -> PyResult<Py<ActionContextItemsIterator>> {
        Py::new(slf.py(),ActionContextItemsIterator::reversed(slf.map.clone()))
    }

    pub fn clear(&mut self) {
        self.map.lock().unwrap().clear()
    }

    pub fn copy(&self) -> Self {
        Self{map: Arc::new(Mutex::new(self.map.lock().unwrap().clone()))}
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.map.lock().unwrap().get(key).map(|a|a.into())
    }

    pub fn has_key(&self, k: &str) -> bool {
        self.map.lock().unwrap().contains_key(k)
    }

    fn items(&self) -> ActionContextItemsView {
        ActionContextItemsView::from(self)
    }

    fn keys(&self) -> ActionContextKeysView {
        ActionContextKeysView::from(self)
    }

    #[args(default = "None")]
    fn pop(&self, key: &str, default: Option<&str>) -> PyResult<String> {
        match self.map.lock().unwrap().remove(key) {
            Some(val) => Ok(val.into()),
            None => match default {
                Some(val) => Ok(val.into()),
                None => Err(PyErr::new::<PyKeyError, _>("Tried to pop on an empty ActionContext and no default"))
            }
        }
    }

    fn popitem(&mut self) -> PyResult<(String, String)> {
        match self.map.lock().unwrap().keys().last() {
            Some(k) => Ok(self.map.lock().unwrap().remove_entry(k).unwrap()),
            None => Err(PyErr::new::<PyKeyError, _>("Tried to 'popitem' on an empty ActionContext"))
        }        
    }

    fn setdefault(&mut self, key:&str, default: &str) -> String {
        match self.get(key) {
            Some(s) => s.into(),
            None => {self.set(key.into(), default.into());default.into()}
        }
    }

    fn update(&mut self, args: &PyTuple, kwargs: Option<&PyDict>) -> PyResult<()>{
        fn extend(map: &mut HashMap<String, String>, dict: &PyDict) -> PyResult<()> {
            for (key, value) in dict {
                let key: String = key.extract()?;
                let value: String = value.extract()?;

                map.insert(key, value);
            }
            Ok(())
        }

        if args.len() > 1  {
            return Err(PyErr::new::<PyAttributeError, _>("Only one argument was expected"));
        }
        else if args.len() == 1 {
            match args.get_item(0).downcast::<PyDict>() {
                Ok(dict) => {extend(&mut self.map.lock().unwrap(),dict)?},
                Err(e) => {
                    Err(e)?
                }
            }
        }

        if let Some(dict) = kwargs {
            extend(&mut self.map.lock().unwrap(), dict)?;
        }

        Ok(())
    }

    fn values(&self) -> ActionContextValuesView {
        ActionContextValuesView::from(self)
    }
}