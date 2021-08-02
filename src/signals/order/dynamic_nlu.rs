// Standard library
use std::sync::Mutex;

// This crate
use crate::exts::LockIt;

// Other crates
use anyhow::Result;
use lazy_static::lazy_static;
use tokio::sync::mpsc;
use unic_langid::LanguageIdentifier;

lazy_static! {
    pub static ref ENTITY_ADD_CHANNEL: Mutex<Option<mpsc::Sender<EntityAddValueRequest>>> =  Mutex::new(None);
}

pub struct EntityAddValueRequest {
    pub skill: String,
    pub entity: String,
    pub value: String,
    pub langs: Vec<LanguageIdentifier>,
}
pub fn init_dynamic_entities() -> Result<mpsc::Receiver<EntityAddValueRequest>> {
    let (producer, consumer) = mpsc::channel(100);

    (*ENTITY_ADD_CHANNEL.lock_it()) = Some(producer);

    Ok(consumer)
}