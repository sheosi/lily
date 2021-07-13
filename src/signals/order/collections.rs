// Standard library
use std::collections::HashMap;
use std::fmt::Debug;

// This crate
use crate::exts::StringList;
use crate::nlu::{EntityData, EntityDef};
use crate::python::try_translate_all;

// Other crates
use anyhow::Result;
use serde::Deserialize;
use unic_langid::LanguageIdentifier;

fn false_val() -> bool {false}
fn none() -> Option<String> {None}

#[derive(Clone, Debug, Deserialize)]
pub enum Hook {
    #[serde(rename="query")]
    Query(String),
    #[serde(rename="action")]
    Action(String),
    #[serde(rename="signal")]
    Signal(String)
}

fn empty_map() -> HashMap<String, SlotData> {
    HashMap::new()
}

#[derive(Debug, Deserialize)]
pub struct IntentData {

    #[serde(alias = "samples", alias = "sample")]
    pub utts:  StringList,
    #[serde(default="empty_map")]
    pub slots: HashMap<String, SlotData>,
    #[serde(flatten)]
    pub hook: Hook
}

#[derive(Debug, Deserialize)]
pub struct SlotData {
    #[serde(rename="type")]
    pub slot_type: OrderKind,
    #[serde(default="false_val")]
    pub required: bool,
    #[serde(default="none")]
    pub prompt: Option<String>,
    #[serde(default="none")]
    pub reprompt: Option<String>
}

#[derive(Clone, Deserialize)]
struct OrderEntity {
    kind: OrderKind,
    example: String
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum OrderKind {
    Ref(String),
    Def(YamlEntityDef)
}

#[derive(Debug, Clone, Deserialize)]
pub struct YamlEntityDef {
    data: Vec<String>
}

impl YamlEntityDef {
    pub fn try_into_with_trans(self, lang: &LanguageIdentifier) -> Result<EntityDef> {
        let mut data = Vec::new();

        for trans_data in self.data.into_iter(){
            let mut translations = try_translate_all(&trans_data, &lang.to_string())?;
            let value = translations.swap_remove(0);
            data.push(EntityData{value, synonyms: StringList::from_vec(translations)});
        }

        Ok(EntityDef::new(data, true))
    }
}