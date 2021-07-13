// Standard library
use std::collections::HashMap;
use std::fmt::Debug;
use std::mem::replace;
use std::sync::{Arc, Mutex};

// This crate
use crate::actions::{ActionAnswer, ActionContext, ActionSet, MainAnswer};
use crate::config::Config;
use crate::exts::LockIt;
use crate::nlu::{EntityDef, EntityInstance, Nlu, NluManager, NluManagerStatic, NluResponseSlot, NluUtterance};
use crate::python::try_translate;
use crate::stt::DecodeRes;
use crate::signals::{collections::{IntentData, OrderKind}, ActMap, Signal, SignalEventShared};
use crate::vars::{mangle, MIN_SCORE_FOR_ACTION};

// Other crates
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use lazy_static::lazy_static;
use log::{debug, info, error, warn};
use tokio::{select, sync::mpsc};
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