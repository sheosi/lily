// Standard library
use std::{collections::HashMap, sync::Mutex};

// This crate
use crate::exts::LockIt;
use crate::nlu::IntentData;

// Other crates
use anyhow::Result;
use lazy_static::lazy_static;
use tokio::sync::mpsc;
use unic_langid::LanguageIdentifier;

lazy_static! {
    pub static ref DYNAMIC_NLU_CHANNEL: Mutex<Option<mpsc::Sender<DynamicNluRequest>>> =  Mutex::new(None);
}

pub enum DynamicNluRequest {
    AddIntent(AddIntentRequest),
    EntityAddValue(EntityAddValueRequest)
}
pub struct AddIntentRequest {
    pub by_lang: HashMap<LanguageIdentifier, IntentData>,
    pub skill: String,
    pub intent_name: String,
}

pub struct EntityAddValueRequest {
    pub skill: String,
    pub entity: String,
    pub value: String,
    pub langs: Vec<LanguageIdentifier>,
}
pub fn init_dynamic_nlu() -> Result<mpsc::Receiver<DynamicNluRequest>> {
    let (producer, consumer) = mpsc::channel(100);

    (*DYNAMIC_NLU_CHANNEL.lock_it()) = Some(producer);

    Ok(consumer)
}