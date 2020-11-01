

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Serialize;
use fluent_langneg::{negotiate_languages, NegotiationStrategy};
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
    type NluType: Nlu;
    fn ready_lang(&mut self, lang: &LanguageIdentifier) -> Result<()>;

    fn add_intent(&mut self, order_name: &str, phrases: Vec<NluUtterance>);
    fn add_entity(&mut self, name:&str, def: EntityDef);

    // Consume the struct so that we can reuse memory
    fn train(self, train_set_path: &Path, engine_path: &Path, lang: &LanguageIdentifier) -> Result<Self::NluType>;
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
}

pub trait NluManagerConf {
    fn get_paths() -> (PathBuf, PathBuf);
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


