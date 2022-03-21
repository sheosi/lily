use std::collections::HashMap;
use std::fmt::Debug;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::signals::collections::Hook;
use crate::vars::mangle;

use anyhow::Result;
use async_trait::async_trait;
use fluent_langneg::{negotiate_languages, NegotiationStrategy};
use serde::Serialize;
use unic_langid::LanguageIdentifier;

#[cfg(not(feature="devel_rasa_nlu"))]
mod snips;
#[cfg(not(feature="devel_rasa_nlu"))]
pub use self::snips::*;

#[cfg(feature="devel_rasa_nlu")]
mod rasa;
#[cfg(feature="devel_rasa_nlu")]
pub use self::rasa::*;

pub trait NluManager {
    type NluType: Nlu + Debug + Send;
    fn ready_lang(&mut self, lang: &LanguageIdentifier) -> Result<()>;

    fn add_intent(&mut self, order_name: &str, phrases: Vec<NluUtterance>);
    fn add_entity(&mut self, name: String, def: EntityDef);
    fn add_entity_value(&mut self, name: &str, value: String) -> Result<()>;

    // Consume the struct so that we can reuse memory
    fn train(&self, train_set_path: &Path, engine_path: &Path, lang: &LanguageIdentifier) -> Result<Self::NluType>;
}

pub trait NluManagerStatic {
    fn new() -> Self;
    fn list_compatible_langs() -> Vec<LanguageIdentifier>;
    fn is_lang_compatible(lang: &LanguageIdentifier) -> bool {
        !negotiate_languages(&[lang],
            &Self::list_compatible_langs(),
            None,
            NegotiationStrategy::Filtering
        ).is_empty()
    }
    fn name() -> &'static str;
    fn get_paths() -> (PathBuf, PathBuf);
}

#[derive(Clone,Debug)]
pub enum NluUtterance{
    Direct(String),
    WithEntities {text: String, entities: HashMap<String, EntityInstance>}
}

#[async_trait(?Send)]
pub trait Nlu {
    async fn parse(&self, input: &str) -> Result<NluResponse>;
}

#[derive(Clone, Debug)]
pub struct EntityInstance {
    pub kind: String,
    pub example: String
}

#[derive(Clone, Debug, Serialize)]
pub struct EntityData {
    pub value: String,
    #[serde(default)]
    pub synonyms: Vec<String>
}


#[derive(Clone, Debug)]
pub struct EntityDef {
    pub data: Vec<EntityData>,
    pub automatically_extensible: bool
}


impl EntityDef {
    pub fn new(data: Vec<EntityData>, automatically_extensible: bool) -> Self {
        Self {data, automatically_extensible}
    }
}
#[derive(Debug, Clone)]
pub enum OrderKind {
    Ref(String),
    Def(EntityDef)
}

#[derive(Clone, Debug)]
pub struct IntentData {
    pub utts:  Vec<String>,
    pub slots: HashMap<String, SlotData>,
    pub hook: Hook
}

impl IntentData {
    pub fn into_utterances(self, skill_name: &str)  -> Vec<NluUtterance> {
        let mut slots_res:HashMap<String, EntityInstance> = HashMap::new();
        for (slot_name, slot_data) in self.slots.iter() {

            // Handle that slot types might be defined on the spot
            let (ent_kind_name, example):(_, String) = match slot_data.slot_type.clone() {
                OrderKind::Ref(name) => (name, "".into()),
                OrderKind::Def(def) => {
                    
                    let name = mangle(skill_name, slot_name);
                    let example = def.data.first().as_ref().map(|d|d.value.clone()).unwrap_or("".into());
                    (name, example)
                }
            };

            slots_res.insert(
                slot_name.to_string(),
                EntityInstance {
                    kind: ent_kind_name,
                    example,
                },
            );
        }
        self.utts.into_iter().map(|utt|
            if slots_res.is_empty() {
                NluUtterance::Direct(utt)
            }
            else {
                NluUtterance::WithEntities {
                    text: utt,
                    entities: slots_res.clone(),
                }
        }).collect()
    }
}

#[derive(Clone, Debug)]
pub struct SlotData {
    pub slot_type: OrderKind,
    pub required: bool,

    // In case this slot is not present in the user response but is required
    // have a way of automatically asking for it
    pub prompt: Option<String>,

    // Second chance for asking the user for this slot
    pub reprompt: Option<String>
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


pub fn try_open_file_and_check(path: &Path, new_contents: &str) -> Result<Option<std::fs::File>, std::io::Error> {
    let file = std::fs::OpenOptions::new().read(true).write(true).create(true).open(path);

    if let Ok(mut file) = file {
        let mut old_file: String = String::new();
        file.read_to_string(&mut old_file)?;
        if old_file != new_contents {
            Ok(Some(file))
        }
        else {
            Ok(None)
        }
    }
    else {
        if let Some(path_parent) = path.parent() {
            std::fs::create_dir_all(path_parent)?;
        }
        let file = std::fs::OpenOptions::new().read(true).write(true).create(true).open(path)?;

        Ok(Some(file))
    }

}

pub fn write_contents(file: &mut File, contents: &str) -> Result <()> {
    file.set_len(0)?; // Truncate file
    file.seek(SeekFrom::Start(0))?; // Start from the start
    file.write_all(contents[..].as_bytes())?;
    file.sync_all()?;

    Ok(())
}

pub fn compare_sets_and_train<F: FnOnce()>(train_set_path: &Path, train_set:&str, engine_path: &Path, callback: F) -> Result<()> {
    if let Some(mut train_file) = try_open_file_and_check(train_set_path, train_set)? {
        // Create parents
        if let Some(path_parent) = engine_path.parent() {
            std::fs::create_dir_all(path_parent)?;
        }

        // Clean engine folder
        if engine_path.is_dir() {
            std::fs::remove_dir_all(engine_path)?;
        }

        // Write train file
        write_contents(&mut train_file, train_set)?;

        // Train engine
        callback();

    }

    Ok(())
}


