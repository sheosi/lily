// Standard library
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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

    pub fn as_json_value(&self) -> Option<serde_json::Value> {
        match self {
            DictElement::String(s) => Some(serde_json::Value::String(s.to_string())),
            _ => None
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum DictElement {
    String(String)
}