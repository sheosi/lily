// Standard library
use std::collections::HashMap;

/** A struct holding all the data for a skill to answer a user request. Note that
 * is it made in a way that would resemble an output JSON-like message */
pub struct ActionContext {
    pub locale: String,
    pub satellite: Option<SatelliteData>,
    pub data: ContextData,
}

pub struct SatelliteData {
    pub uuid: String,
}

pub enum ContextData {
    Event { event: String },
    Intent { intent: IntentData },
}

impl ContextData {
    pub fn as_intent(&self) -> Option<&IntentData> {
        match self {
            ContextData::Intent { intent } => Some(intent),
            _ => None,
        }
    }
}

pub struct IntentData {
    pub input: String,
    pub name: String,
    pub confidence: f32,
    pub slots: HashMap<String, String>,
}
