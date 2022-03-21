// Standard library
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

// This crate
use crate::exts::LockIt;
#[cfg(feature="python_skills")]
use crate::vars::UNEXPECTED_MSG;

// Other crates
#[cfg(feature="python_skills")]
use pyo3::{IntoPy, Py, PyAny, PyErr, PyIterProtocol, PyObject, PyRef, PyRefMut, PyResult, Python, exceptions::*, types::{PyDict, PyIterator, PyTuple, PyType}};
#[cfg(feature="python_skills")]
use pyo3::prelude::{FromPyObject, pyclass, pymethods, pyproto};
#[cfg(feature="python_skills")]
use pyo3::class::PyObjectProtocol;
#[cfg(feature="python_skills")]
use pyo3::mapping::PyMappingProtocol;
#[cfg(feature="python_skills")]
use pyo3::sequence::PySequenceProtocol;

#[cfg(feature="python_skills")]
#[pyclass]
pub struct ActionContext {
    pub locale: String,
    pub satellite: Option<SatelliteData>,
    pub data: ContextData,
}

#[cfg(not(feature="python_skills"))]
/** A struct holding all the data for a skill to answer a user request. Note that
 * is it made in a way that would resemble an output JSON-like message */
pub struct ActionContext {
    pub locale: String,
    pub satellite: Option<SatelliteData>,
    pub data: ContextData,
}

#[cfg(not(feature="python_skills"))]
pub struct SatelliteData {
    pub uuid: String   
}

#[cfg(feature="python_skills")]
#[pyclass]
pub struct SatelliteData {
    pub uuid: String   
}

pub enum ContextData {
    Event{event: String},
    Intent{intent: IntentData},
}

#[cfg(feature="python_skills")]
#[pyclass]
pub struct ContextDataPy {
    context: ContextData
}

impl ContextData {
    pub fn as_intent(&self) -> Option<&IntentData> {
        match self {
            ContextData::Intent{intent} => Some(intent),
            _ => None
        }
    }
}

#[cfg(not(feature="python_skills"))]
pub struct IntentData {
    pub input: String,
    pub name: String,
    pub confidence: f32,
    pub slots: DynamicDict,
}

#[cfg(feature="python_skills")]
#[pyclass]
pub struct IntentData {
    pub input: String,
    pub name: String,
    pub confidence: f32,
    pub slots: DynamicDict,
}

#[cfg(feature="python_skills")]
#[pyclass]
#[derive(Debug, Clone)]
pub struct DynamicDict {
    pub map: Arc<Mutex<HashMap<String, DictElement>>>,
}

#[cfg(not(feature="python_skills"))]
#[derive(Debug, Clone)]
/// Just a basic dictionary implementation, this is used for compatibility both
/// with Python and Rust
pub struct DynamicDict {
    pub map: Arc<Mutex<HashMap<String, DictElement>>>,
}

impl DynamicDict {
    pub fn new() -> Self {
        Self{map: Arc::new(Mutex::new(HashMap::new()))}
    }

    pub fn set_str(&mut self, key: String, value: String) {
        self.map.lock_it().insert(key, DictElement::String(value));
    }

    pub fn set_dict(&mut self, key: String, value: DynamicDict) {
        self.map.lock_it().insert(key, DictElement::Dict(value));
    }

    pub fn set_decimal(&mut self, key: String, value: f32) {
        self.map.lock_it().insert(key, DictElement::Decimal(value));
    }
}

#[cfg(not(feature = "python_skills"))]
impl DynamicDict {
    pub fn copy(&self) -> Self {
        Self{map: Arc::new(Mutex::new(self.map.lock_it().clone()))}
    }

    pub fn get(&self, key: &str) -> Option<DictElement> {
        self.map.lock_it().get(key).cloned()
    }
}

impl PartialEq for DynamicDict {
    fn eq(&self, other: &Self) -> bool {
        *self.map.lock().unwrap() == *other.map.lock().unwrap()
    }
}

#[cfg(feature="python_skills")]
#[derive(Clone, Debug, FromPyObject, PartialEq)]
pub enum DictElement {
    String(String),
    Dict(DynamicDict),
    Decimal(f32)
}

impl DictElement {
    pub fn as_string(&self) -> Option<&str> {
        match self {
            DictElement::String(s) => Some(s),
            _ => None
        }
    }

    pub fn as_dict(&self) -> Option<&DynamicDict> {
        match self {
            DictElement::Dict(d) => Some(d),
            _ => None
        }
    }

    pub fn as_json_value(&self) -> Option<serde_json::Value> {
        match self {
            DictElement::String(s) => Some(serde_json::Value::String(s.to_string())),
            DictElement::Decimal(d) => Some(serde_json::Value::Number(serde_json::Number::from_f64((*d).into()).unwrap())),
            //DictElement::Dict(d) => Some(serde_json::Value::Object(d.map)), // TODO!
            _ => None
        }
    }
}

#[cfg(not(feature="python_skills"))]
#[derive(Clone, Debug, PartialEq)]
pub enum DictElement {
    String(String),
    Dict(DynamicDict),
    Decimal(f32)
}

/***** Python classes *********************************************************/

#[cfg(feature="python_skills")]
impl IntoPy<PyObject> for DictElement {
    fn into_py(self, py: Python) -> PyObject {
        match self {
            DictElement::String(str)=>{str.into_py(py)},
            DictElement::Dict(c) =>{c.into_py(py)},
            DictElement::Decimal(f)=>{f.into_py(py)}
        }
    }
}

#[cfg(feature="python_skills")]
struct BaseIterator {
    inner: Arc<Mutex<HashMap<String, DictElement>>>,
    count: usize,
    reversed: bool
}

#[cfg(feature="python_skills")]
impl BaseIterator {
    fn new(inner: Arc<Mutex<HashMap<String, DictElement>>>) -> Self {
        Self {
            inner,
            count: 0,
            reversed: false
        }
    }

    fn reversed(inner: Arc<Mutex<HashMap<String, DictElement>>>) -> Self {
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
    fn get_inner(&self) -> MutexGuard<HashMap<String, DictElement>> {
        self.inner.lock_it()
    }
}

#[cfg(feature="python_skills")]
#[pyclass]
pub struct DynamicDictItemsIterator {
    base: BaseIterator
}

#[cfg(feature="python_skills")]
impl DynamicDictItemsIterator {
    fn new(inner: Arc<Mutex<HashMap<String, DictElement>>>)-> Self {
        Self { base: BaseIterator::new(inner)}
    }

    fn reversed(inner: Arc<Mutex<HashMap<String, DictElement>>>)-> Self {
        Self { base: BaseIterator::reversed(inner)}
    }
}

#[cfg(feature="python_skills")]
#[pyproto]
impl PyIterProtocol for DynamicDictItemsIterator {
    fn __iter__(slf: PyRefMut<Self>) -> Py<DynamicDictItemsIterator> {
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

#[cfg(feature="python_skills")]
#[pyclass]
struct DynamicDictValuesIterator {
    base: BaseIterator
}

#[cfg(feature="python_skills")]
impl DynamicDictValuesIterator {
    fn new(inner: Arc<Mutex<HashMap<String, DictElement>>>)-> Self {
        Self { base: BaseIterator::new(inner)}
    }

    fn reversed(inner: Arc<Mutex<HashMap<String, DictElement>>>)-> Self {
        Self { base: BaseIterator::reversed(inner)}
    }
}

#[cfg(feature="python_skills")]
#[pyproto]
impl PyIterProtocol for DynamicDictValuesIterator {
    fn __iter__(slf: PyRefMut<Self>) -> Py<DynamicDictValuesIterator> {
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

#[cfg(feature="python_skills")]
#[pyclass]
struct DynamicDictKeysIterator {
    base: BaseIterator
}

#[cfg(feature="python_skills")]
impl DynamicDictKeysIterator {
    fn new(inner: Arc<Mutex<HashMap<String, DictElement>>>)-> Self {
        Self { base: BaseIterator::new(inner)}
    }

    fn reversed(inner: Arc<Mutex<HashMap<String, DictElement>>>)-> Self {
        Self { base: BaseIterator::reversed(inner)}
    }
}

#[cfg(feature="python_skills")]
#[pyproto]
impl PyIterProtocol for DynamicDictKeysIterator {
    fn __iter__(slf: PyRefMut<Self>) -> Py<DynamicDictKeysIterator> {
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

#[cfg(feature="python_skills")]
#[pyclass]
struct DynamicDictItemsView {
    inner: Arc<Mutex<HashMap<String, DictElement>>>,
}

#[cfg(feature="python_skills")]
impl DynamicDictItemsView {
    fn from(ctx: &DynamicDict) -> Self {
        Self{inner: ctx.map.clone()}
    }
}

#[cfg(feature="python_skills")]
#[pymethods]
impl DynamicDictItemsView {
    fn __len__(&self) -> usize {
        self.inner.lock_it().len()
    }
    
    fn __iter__(slf: PyRef<Self>) -> PyResult<Py<DynamicDictItemsIterator>> {
        Py::new(slf.py(),DynamicDictItemsIterator::new(slf.inner.clone()))
    }

    fn __reversed__(slf: PyRef<Self>) -> PyResult<Py<DynamicDictItemsIterator>> {
        Py::new(slf.py(),DynamicDictItemsIterator::reversed(slf.inner.clone()))
    }

}

#[cfg(feature="python_skills")]
#[pyproto]
impl PyIterProtocol for  DynamicDictItemsView {
    fn __iter__(slf: PyRef<Self>) -> PyResult<Py<DynamicDictItemsIterator>> {
        Py::new(slf.py(),DynamicDictItemsIterator::new(slf.inner.clone()))
    }
}

#[cfg(feature="python_skills")]
#[pyproto]
impl PySequenceProtocol for DynamicDictItemsView {
    fn __contains__(&self, k: &str) -> bool {
        self.inner.lock_it().contains_key(k)
    }
}

#[cfg(feature="python_skills")]
#[pyclass]
struct DynamicDictValuesView {
    inner: Arc<Mutex<HashMap<String, DictElement>>>,
}

#[cfg(feature="python_skills")]
impl DynamicDictValuesView {
    fn from(ctx: &DynamicDict) -> Self {
        Self{inner: ctx.map.clone()}
    }
}

#[cfg(feature="python_skills")]
#[pymethods]
impl DynamicDictValuesView {
    fn __len__(&self) -> usize {
        self.inner.lock_it().len()
    }
    
    fn __iter__(slf: PyRef<Self>) -> PyResult<Py<DynamicDictValuesIterator>> {
        Py::new(slf.py(),DynamicDictValuesIterator::new(slf.inner.clone()))
    }

    fn __reversed__(slf: PyRef<Self>) -> PyResult<Py<DynamicDictValuesIterator>> {
        Py::new(slf.py(),DynamicDictValuesIterator::reversed(slf.inner.clone()))
    }

}

#[cfg(feature="python_skills")]
#[pyclass]
struct DynamicDictKeysView {
    inner: Arc<Mutex<HashMap<String, DictElement>>>,
}

#[cfg(feature="python_skills")]
impl DynamicDictKeysView {
    fn from(ctx: &DynamicDict) -> Self {
        Self{inner: ctx.map.clone()}
    }
}

#[cfg(feature="python_skills")]
#[pymethods]
impl DynamicDictKeysView {
    fn __len__(&self) -> usize {
        self.inner.lock_it().len()
    }
    
    fn __iter__(slf: PyRef<Self>) -> PyResult<Py<DynamicDictKeysIterator>> {
        Py::new(slf.py(),DynamicDictKeysIterator::new(slf.inner.clone()))
    }

    fn __reversed__(slf: PyRef<Self>) -> PyResult<Py<DynamicDictKeysIterator>> {
        Py::new(slf.py(),DynamicDictKeysIterator::reversed(slf.inner.clone()))
    }

}

#[cfg(feature="python_skills")]
#[pyproto]
impl PyMappingProtocol for DynamicDict {
    fn __delitem__(&mut self, key: &str) -> PyResult<()> {
        self.map.lock_it().remove(key).ok_or(PyErr::new::<PyAttributeError, _>(
            format!("Key: {} was not found in context", key)
        ))?;
        Ok(())
    }

    fn __getitem__(&self, key: &str) -> PyResult<DictElement> {
        self.get(key).ok_or(PyErr::new::<PyAttributeError, _>(
            format!("Key: {} was not found in context", key)
        ))
    }

    fn __len__(&self) -> usize {
        self.map.lock_it().len()
    }

    fn __setitem__(&mut self, key: String, item: DictElement) {
        self.map.lock_it().insert(key, item);
    }
}

#[cfg(feature="python_skills")]
#[pyproto]
impl PyObjectProtocol for DynamicDict {
    fn __repr__(&self) -> String {
        // TODO: Maybe improve this one and make it more Pythonic
        format!("{:?}",self.map)
    }

    fn __str__(&self) -> String {
        // TODO: Maybe improve this one and make it more Pythonic
        format!("{:?}",self.map)
    }
}

#[cfg(feature="python_skills")]
#[pyproto]
impl PySequenceProtocol for DynamicDict {
    fn __contains__(&self, k: &str) -> bool {
        self.map.lock_it().contains_key(k)
    }
}

#[cfg(feature="python_skills")]
#[pyproto]
impl PyIterProtocol for  DynamicDict {
    fn __iter__(slf: PyRef<Self>) -> PyResult<Py<DynamicDictItemsIterator>> {
        Py::new(slf.py(),DynamicDictItemsIterator::new(slf.map.clone()))
    }
}

#[cfg(feature="python_skills")]
#[pymethods]
impl DynamicDict {
    // classmethods
    #[classmethod]
    fn fromkeys(_cls: &PyType, py:Python, iterable: &PyAny, value: DictElement) -> PyResult<DynamicDict> {
        let mut map = HashMap::new();
        let it = PyIterator::from_object(py,iterable.call_method("__iter__", (), None)?)?;
        for key in it {
            let key: String = key?.extract()?;
            map.insert(key,value.clone());
        }

        Ok(Self {map: Arc::new(Mutex::new(map))})
    }

    fn __eq__(&mut self, other: &PyAny) -> bool {
        match other.extract::<DynamicDict>() {
            Ok(c) => *self.map.lock_it() == *c.map.lock_it(),
            Err(_) => false
        }
    }

    fn __lt__(&mut self, other: PyRef<DynamicDict>) -> bool {
        self.map.lock_it().len() < other.map.lock_it().len()
    }

    fn __reversed__(slf: PyRef<Self>) -> PyResult<Py<DynamicDictItemsIterator>> {
        Py::new(slf.py(),DynamicDictItemsIterator::reversed(slf.map.clone()))
    }

    pub fn clear(&mut self) {
        self.map.lock_it().clear()
    }

    pub fn copy(&self) -> Self {
        Self{map: Arc::new(Mutex::new(self.map.lock_it().clone()))}
    }

    pub fn get(&self, key: &str) -> Option<DictElement> {
        self.map.lock_it().get(key).cloned()
    }

    pub fn has_key(&self, k: &str) -> bool {
        self.map.lock_it().contains_key(k)
    }

    fn items(&self) -> DynamicDictItemsView {
        DynamicDictItemsView::from(self)
    }

    fn keys(&self) -> DynamicDictKeysView {
        DynamicDictKeysView::from(self)
    }

    #[args(default = "None")]
    fn pop(&self, key: &str, default: Option<DictElement>) -> PyResult<DictElement> {
        match self.map.lock_it().remove(key) {
            Some(val) => Ok(val.into()),
            None => match default {
                Some(val) => Ok(val.into()),
                None => Err(PyErr::new::<PyKeyError, _>("Tried to pop on an empty DynamicDict and no default"))
            }
        }
    }

    fn popitem(&mut self) -> PyResult<(String, DictElement)> {
        match self.map.lock_it().keys().last() {
            Some(k) => {
                Ok(self.map.lock_it().remove_entry(k).expect(UNEXPECTED_MSG))
            },
            None => Err(PyErr::new::<PyKeyError, _>("Tried to 'popitem' on an empty DynamicDict"))
        }        
    }

    fn setdefault(&mut self, key:&str, default: DictElement) -> DictElement {
        match self.get(key) {
            Some(s) => s.into(),
            None => {self.map.lock_it().insert(key.into(), default);default.into()}
        }
    }

    fn update(&mut self, args: &PyTuple, kwargs: Option<&PyDict>) -> PyResult<()>{
        fn extend(map: &mut HashMap<String, DictElement>, dict: &PyDict) -> PyResult<()> {
            for (key, value) in dict {
                let key: String = key.extract()?;
                let value: DictElement = value.extract()?;

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

    fn values(&self) -> DynamicDictValuesView {
        DynamicDictValuesView::from(self)
    }
}