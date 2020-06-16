mod snips;
pub use self::snips::*;

use std::path::Path;
use std::collections::HashMap;
use unic_langid::LanguageIdentifier;
use anyhow::Result;
use serde::Serialize;

#[cfg(feature="devel_rasa_nlu")]
mod rasa;
#[cfg(feature="devel_rasa_nlu")]
pub use self::rasa::*;

pub trait NluManager {
    fn add_intent(&mut self, order_name: &str, phrases: Vec<NluUtterance>);
    fn add_entity(&mut self, name:&str, def: EntityDef);

    // Consume the struct so that we can reuse memory
    fn train(self, train_set_path: &Path, engine_path: &Path, lang: &LanguageIdentifier) -> Result<()>; 
}

pub enum NluUtterance{
    Direct(String),
    WithEntities {text: String, entities: HashMap<String, EntityInstance>}
}

pub trait Nlu {
    fn parse(&self, input: &str) -> Result<NluResponse>;
}

#[derive(Clone)]
pub struct EntityInstance {
    pub kind: String,
    pub example: String
}

#[derive(Serialize)]
pub struct EntityData {
    pub value: String,
    pub synonyms: Vec<String>
}

#[derive(Serialize)]
pub struct EntityDef {
    //Note: This was made for Snips, but Rasa is a bit different, use_synonyms
    // and automatically_extensible doesn't exist. Also uses just one data
    pub data: Vec<EntityData>,
    pub use_synonyms: bool,
    pub automatically_extensible: bool
}


#[derive(Debug)]
pub struct NluResponse {
    pub name: Option<String>,
    pub confidence: f32,
    pub slots: Vec<NluResponseSlot>
}

#[derive(Debug)]
pub struct NluResponseSlot {
    pub value: String,
    pub name: String
}