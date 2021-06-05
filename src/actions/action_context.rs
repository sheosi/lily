// Standard library
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

// This crate
use crate::exts::LockIt;
use crate::vars::UNEXPECTED_MSG;

// Other crates
use pyo3::{IntoPy, Py, PyAny, PyErr, PyIterProtocol, PyObject, PyRef, PyRefMut, PyResult, Python, exceptions::*, types::{PyDict, PyIterator, PyTuple, PyType}};
use pyo3::prelude::{FromPyObject, pyclass, pymethods, pyproto};
use pyo3::class::PyObjectProtocol;
use pyo3::mapping::PyMappingProtocol;
use pyo3::sequence::PySequenceProtocol;


#[derive(Clone, Debug, FromPyObject, PartialEq)]
pub enum ContextElement {
    String(String),
    Dict(ActionContext)
}

impl IntoPy<PyObject> for ContextElement {
    fn into_py(self, py: Python) -> PyObject {
        match self {
            ContextElement::String(str)=>{str.into_py(py)},
            ContextElement::Dict(c) =>{c.into_py(py)}
        }
    }
}


struct BaseIterator {
    inner: Arc<Mutex<HashMap<String, ContextElement>>>,
    count: usize,
    reversed: bool
}

impl BaseIterator {
    fn new(inner: Arc<Mutex<HashMap<String, ContextElement>>>) -> Self {
        Self {
            inner,
            count: 0,
            reversed: false
        }
    }

    fn reversed(inner: Arc<Mutex<HashMap<String, ContextElement>>>) -> Self {
        Self {
            count: inner.lock_it().len(),
            inner: inner.clone(),
            reversed: true
        }
    }

    fn is_end(&self) -> bool {
        if self.reversed{self.count == 0}
        else {self.count == self.inner.lock_it().len()}
    }

    fn count(&mut self) -> usize {
        let old_count = self.count;
        self.count = if self.reversed{self.count -1} else{self.count + 1};
        old_count
    }
    fn get_inner(&self) -> MutexGuard<HashMap<String, ContextElement>> {
        self.inner.lock_it()
    }
}


#[pyclass]
pub struct ActionContextItemsIterator {
    base: BaseIterator
}

impl ActionContextItemsIterator {
    fn new(inner: Arc<Mutex<HashMap<String, ContextElement>>>)-> Self {
        Self { base: BaseIterator::new(inner)}
    }

    fn reversed(inner: Arc<Mutex<HashMap<String, ContextElement>>>)-> Self {
        Self { base: BaseIterator::reversed(inner)}
    }
}

#[pyproto]
impl PyIterProtocol for ActionContextItemsIterator {
    fn __iter__(slf: PyRefMut<Self>) -> Py<ActionContextItemsIterator> {
        slf.into()
    }

    
    fn __next__(mut slf: PyRefMut<Self>) -> Option<(String, PyObject)> {
        if !slf.base.is_end(){
            let count = slf.base.count();
            slf.base.get_inner().iter().nth(count).map(|(a,b)|{(a.into(), b.clone().into_py(slf.py()))})
        }
        else {None}
    }
}

#[pyclass]
struct ActionContextValuesIterator {
    base: BaseIterator
}

impl ActionContextValuesIterator {
    fn new(inner: Arc<Mutex<HashMap<String, ContextElement>>>)-> Self {
        Self { base: BaseIterator::new(inner)}
    }

    fn reversed(inner: Arc<Mutex<HashMap<String, ContextElement>>>)-> Self {
        Self { base: BaseIterator::reversed(inner)}
    }
}

#[pyproto]
impl PyIterProtocol for ActionContextValuesIterator {
    fn __iter__(slf: PyRefMut<Self>) -> Py<ActionContextValuesIterator> {
        slf.into()
    }

    
    fn __next__(mut slf: PyRefMut<Self>) -> Option<PyObject> {
        if !slf.base.is_end(){
            let count = slf.base.count();
            slf.base.get_inner().values().nth(count).map(|a|a.clone().into_py(slf.py()))
        }
        else {None}
    }
}

#[pyclass]
struct ActionContextKeysIterator {
    base: BaseIterator
}

impl ActionContextKeysIterator {
    fn new(inner: Arc<Mutex<HashMap<String, ContextElement>>>)-> Self {
        Self { base: BaseIterator::new(inner)}
    }

    fn reversed(inner: Arc<Mutex<HashMap<String, ContextElement>>>)-> Self {
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
    inner: Arc<Mutex<HashMap<String, ContextElement>>>,
}

impl ActionContextItemsView {
    fn from(ctx: &ActionContext) -> Self {
        Self{inner: ctx.map.clone()}
    }
}


#[pymethods]
impl ActionContextItemsView {
    fn __len__(&self) -> usize {
        self.inner.lock_it().len()
    }
    
    fn __iter__(slf: PyRef<Self>) -> PyResult<Py<ActionContextItemsIterator>> {
        Py::new(slf.py(),ActionContextItemsIterator::new(slf.inner.clone()))
    }

    fn __reversed__(slf: PyRef<Self>) -> PyResult<Py<ActionContextItemsIterator>> {
        Py::new(slf.py(),ActionContextItemsIterator::reversed(slf.inner.clone()))
    }

}

#[pyproto]
impl PyIterProtocol for  ActionContextItemsView {
    fn __iter__(slf: PyRef<Self>) -> PyResult<Py<ActionContextItemsIterator>> {
        Py::new(slf.py(),ActionContextItemsIterator::new(slf.inner.clone()))
    }
}

#[pyproto]
impl PySequenceProtocol for ActionContextItemsView {
    fn __contains__(&self, k: &str) -> bool {
        self.inner.lock_it().contains_key(k)
    }
}


#[pyclass]
struct ActionContextValuesView {
    inner: Arc<Mutex<HashMap<String, ContextElement>>>,
}

impl ActionContextValuesView {
    fn from(ctx: &ActionContext) -> Self {
        Self{inner: ctx.map.clone()}
    }
}


#[pymethods]
impl ActionContextValuesView {
    fn __len__(&self) -> usize {
        self.inner.lock_it().len()
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
    inner: Arc<Mutex<HashMap<String, ContextElement>>>,
}

impl ActionContextKeysView {
    fn from(ctx: &ActionContext) -> Self {
        Self{inner: ctx.map.clone()}
    }
}

#[pymethods]
impl ActionContextKeysView {
    fn __len__(&self) -> usize {
        self.inner.lock_it().len()
    }
    
    fn __iter__(slf: PyRef<Self>) -> PyResult<Py<ActionContextKeysIterator>> {
        Py::new(slf.py(),ActionContextKeysIterator::new(slf.inner.clone()))
    }

    fn __reversed__(slf: PyRef<Self>) -> PyResult<Py<ActionContextKeysIterator>> {
        Py::new(slf.py(),ActionContextKeysIterator::reversed(slf.inner.clone()))
    }

}

#[pyclass]
#[derive(Debug, Clone)]
pub struct ActionContext {
    map: Arc<Mutex<HashMap<String, ContextElement>>>,
}

impl ActionContext {
    pub fn new() -> Self {
        Self{map: Arc::new(Mutex::new(HashMap::new()))}
    }

    pub fn set(&mut self, key: String, value: ContextElement) {
        self.map.lock_it().insert(key, value);
    }

    pub fn set_str(&mut self, key: String, value: String) {
        self.map.lock_it().insert(key, ContextElement::String(value));
    }

    pub fn set_dict(&mut self, key: String, value: ActionContext) {
        self.map.lock_it().insert(key, ContextElement::Dict(value));
    }

}

impl PartialEq for ActionContext {
    fn eq(&self, other: &Self) -> bool {
        *self.map.lock().unwrap() == *other.map.lock().unwrap()
    }
}

#[pyproto]
impl PyMappingProtocol for ActionContext {
    fn __delitem__(&mut self, key: &str) -> PyResult<()> {
        self.map.lock_it().remove(key).ok_or(PyErr::new::<PyAttributeError, _>(
            format!("Key: {} was not found in context", key)
        ))?;
        Ok(())
    }

    fn __getitem__(&self, key: &str) -> PyResult<ContextElement> {
        self.get(key).ok_or(PyErr::new::<PyAttributeError, _>(
            format!("Key: {} was not found in context", key)
        ))
    }

    fn __len__(&self) -> usize {
        self.map.lock_it().len()
    }

    fn __setitem__(&mut self, key: String, item: ContextElement) {
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
        self.map.lock_it().contains_key(k)
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
    fn fromkeys(_cls: &&PyType, py:Python, iterable: &PyAny, value: ContextElement) -> PyResult<ActionContext> {
        let mut map = HashMap::new();
        let it = PyIterator::from_object(py,iterable.call_method("__iter__", (), None)?)?;
        for key in it {
            let key: String = key?.extract()?;
            map.insert(key,value.clone());
        }

        Ok(Self {map: Arc::new(Mutex::new(map))})
    }

    fn __eq__(&mut self, other: &PyAny) -> bool {
        match other.extract::<ActionContext>() {
            Ok(c) => *self.map.lock_it() == *c.map.lock_it(),
            Err(_) => false
        }
    }

    fn __lt__(&mut self, other: PyRef<ActionContext>) -> bool {
        self.map.lock_it().len() < other.map.lock_it().len()
    }

    fn __reversed__(slf: PyRef<Self>) -> PyResult<Py<ActionContextItemsIterator>> {
        Py::new(slf.py(),ActionContextItemsIterator::reversed(slf.map.clone()))
    }

    pub fn clear(&mut self) {
        self.map.lock_it().clear()
    }

    pub fn copy(&self) -> Self {
        Self{map: Arc::new(Mutex::new(self.map.lock_it().clone()))}
    }

    pub fn get(&self, key: &str) -> Option<ContextElement> {
        self.map.lock_it().get(key).map(|a|a.clone())
    }

    pub fn has_key(&self, k: &str) -> bool {
        self.map.lock_it().contains_key(k)
    }

    fn items(&self) -> ActionContextItemsView {
        ActionContextItemsView::from(self)
    }

    fn keys(&self) -> ActionContextKeysView {
        ActionContextKeysView::from(self)
    }

    #[args(default = "None")]
    fn pop(&self, key: &str, default: Option<ContextElement>) -> PyResult<ContextElement> {
        match self.map.lock_it().remove(key) {
            Some(val) => Ok(val.into()),
            None => match default {
                Some(val) => Ok(val.into()),
                None => Err(PyErr::new::<PyKeyError, _>("Tried to pop on an empty ActionContext and no default"))
            }
        }
    }

    fn popitem(&mut self) -> PyResult<(String, ContextElement)> {
        match self.map.lock_it().keys().last() {
            Some(k) => {
                Ok(self.map.lock_it().remove_entry(k).expect(UNEXPECTED_MSG))
            },
            None => Err(PyErr::new::<PyKeyError, _>("Tried to 'popitem' on an empty ActionContext"))
        }        
    }

    fn setdefault(&mut self, key:&str, default: ContextElement) -> ContextElement {
        match self.get(key) {
            Some(s) => s.into(),
            None => {self.set(key.into(), default.clone().into());default.into()}
        }
    }

    fn update(&mut self, args: &PyTuple, kwargs: Option<&PyDict>) -> PyResult<()>{
        fn extend(map: &mut HashMap<String, ContextElement>, dict: &PyDict) -> PyResult<()> {
            for (key, value) in dict {
                let key: String = key.extract()?;
                let value: ContextElement = value.extract()?;

                map.insert(key, value);
            }
            Ok(())
        }

        if args.len() > 1  {
            return Err(PyErr::new::<PyAttributeError, _>("Only one argument was expected"));
        }
        else if args.len() == 1 {
            match args.get_item(0).downcast::<PyDict>() {
                Ok(dict) => {extend(&mut self.map.lock_it(),dict)?},
                Err(e) => {
                    Err(e)?
                }
            }
        }

        if let Some(dict) = kwargs {
            extend(&mut self.map.lock_it(), dict)?;
        }

        Ok(())
    }

    fn values(&self) -> ActionContextValuesView {
        ActionContextValuesView::from(self)
    }
}