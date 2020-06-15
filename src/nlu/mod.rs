mod snips;
pub use self::snips::*;

use std::path::Path;
use std::collections::HashMap;
use unic_langid::LanguageIdentifier;
use anyhow::Result;

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
    type NluResult;

    fn parse(&self, input: &str) -> snips_nlu_lib::Result<Self::NluResult>;
}
