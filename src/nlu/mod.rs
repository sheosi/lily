use std::collections::HashMap;
use std::fmt::{self, Debug};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::marker::PhantomData;
use std::str::FromStr;
use std::path::{Path, PathBuf};

use crate::python::try_translate;
use crate::signals::StringList;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde::{de::{self, MapAccess, Visitor}, Deserialize, Deserializer, Serialize};
use fluent_langneg::{negotiate_languages, NegotiationStrategy};
use unic_langid::LanguageIdentifier;
use void::Void;

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
    fn add_entity(&mut self, name:&str, def: EntityDef);
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
}

pub trait NluManagerConf {
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
    #[serde(rename = "text")]
    pub value: String,
    #[serde(default, alias = "synonym")]
    pub synonyms: StringList
}

impl EntityData {
    pub fn into_translation(self, lang: &LanguageIdentifier) -> Result<Self> {
        let l_str = lang.to_string();
        let value = try_translate(&self.value, &l_str)?;
        let synonyms = self.synonyms.into_translation(lang)
        .map_err(|v|anyhow!("Translation of '{}' failed", v.join("\"")))?;

        Ok(EntityData {value,synonyms: StringList::from_vec(synonyms)})
    }
}

impl FromStr for EntityData {
    type Err = Void;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(EntityData {
            value: s.to_string(),
            synonyms: StringList::new()
        })
    }
}

impl<'de> Deserialize<'de> for EntityData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        let data = String::deserialize(deserializer)?;
        FromStr::from_str(&data).map_err(de::Error::custom)
    }
}

fn string_or_struct<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: Deserialize<'de> + FromStr<Err = Void>,
    D: Deserializer<'de>,
{
        // This is a Visitor that forwards string types to T's `FromStr` impl and
    // forwards map types to T's `Deserialize` impl. The `PhantomData` is to
    // keep the compiler from complaining about T being an unused generic type
    // parameter. We need T in order to know the Value type for the Visitor
    // impl.
    struct StringOrStruct<T>(PhantomData<fn() -> T>);

    impl<'de, T> Visitor<'de> for StringOrStruct<T>
    where
        T: Deserialize<'de> + FromStr<Err = Void>,
    {
        type Value = T;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("string or map")
        }

        fn visit_str<E>(self, value: &str) -> Result<T, E>
        where
            E: de::Error,
        {
            Ok(FromStr::from_str(value).unwrap())
        }

        fn visit_map<M>(self, map: M) -> Result<T, M::Error>
        where
            M: MapAccess<'de>,
        {
            // `MapAccessDeserializer` is a wrapper that turns a `MapAccess`
            // into a `Deserializer`, allowing it to be used as the input to T's
            // `Deserialize` implementation. T then deserializes itself using
            // the entries from the map visitor.
            Deserialize::deserialize(de::value::MapAccessDeserializer::new(map))
        }
    }

    deserializer.deserialize_any(StringOrStruct(PhantomData))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EntityDef {
    //Note: This was made for Snips, but Rasa is a bit different
    // automatically_extensible don't exist. Also uses just one data
    pub data: Vec<EntityData>,
    #[serde(alias="accept_others")]
    pub automatically_extensible: bool
}

impl EntityDef {
    pub fn into_translation(self, lang: &LanguageIdentifier) -> Result<EntityDef> {
        let data_res: Result<Vec<_>,_> = self.data.into_iter().map(|d|d.into_translation(lang)).collect();
        Ok(EntityDef {
            data: data_res?,
            automatically_extensible: self.automatically_extensible
        })
    }
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


