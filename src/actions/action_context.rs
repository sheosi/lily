// Standard library
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

// This crate
use crate::exts::LockIt;

/** A struct holding all the data for a skill to answer a user request. Note that
 * is it made in a way that would resemble an output JSON-like message */
pub struct ActionContext {
    pub locale: String,
    pub satellite: Option<SatelliteData>,
    pub data: ContextData,
}

pub struct SatelliteData {
    pub uuid: String   
}

pub enum ContextData {
    Event{event: String},
    Intent{intent: IntentData},
}

impl ContextData {
    pub fn as_intent(&self) -> Option<&IntentData> {
        match self {
            ContextData::Intent{intent} => Some(intent),
            _ => None
        }
    }
}

pub struct IntentData {
    pub input: String,
    pub name: String,
    pub confidence: f32,
    pub slots: DynamicDict,
}

#[derive(Debug, Clone)]
/// Just a basic dictionary implementation that was used for compatibility both
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

#[derive(Clone, Debug, PartialEq)]
pub enum DictElement {
    String(String),
    Dict(DynamicDict),
    Decimal(f32)
}