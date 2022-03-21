mod action_context;

pub use self::action_context::*;

// Standard library
use std::fmt; // For Debug in LocalActionRegistry
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};

// This crate
use crate::collections::BaseRegistry;
use crate::exts::LockIt;

// Other crates
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use lazy_static::lazy_static;
use log::error;
use lily_common::audio::Audio;

pub type ActionRegistry = BaseRegistry<dyn Action + Send>;
pub type ActionItem = Arc<Mutex<dyn Action + Send>>;

lazy_static! {
    pub static ref ACT_REG: Mutex<ActionRegistry> = Mutex::new(ActionRegistry::new());
}

#[derive(Clone)]
pub enum MainAnswer {
    Sound(Audio),
    Text(String)
}

#[derive(Clone)]
pub struct ActionAnswer {
    pub answer: MainAnswer,
    pub should_end_session: bool
}

impl ActionAnswer {
    pub fn audio_file(path: &Path, end_session: bool) -> Result<Self> {
        let mut f = File::open(path)?;
        let mut buffer = vec![0; fs::metadata(path)?.len() as usize];
        f.read(&mut buffer)?;
        let a = Audio::new_encoded(buffer);
        Ok(Self {answer: MainAnswer::Sound(a), should_end_session: end_session})
    }

    pub fn send_text(text: String, end_session: bool) -> Result<Self> {
        Ok(Self {answer: MainAnswer::Text(text), should_end_session: end_session})
    }
}



#[async_trait(?Send)]
pub trait Action {
    async fn call(&self, context: &ActionContext) -> Result<ActionAnswer>;
    fn get_name(&self) -> String;
}

pub trait ActionItemExt {
    fn new<A: Action + Send + 'static>(act: A) -> ActionItem;
}

impl ActionItemExt for ActionItem {
    fn new<A: Action + Send + 'static>(act: A) -> ActionItem {
        Arc::new(Mutex::new(act))
    }
}


#[derive(Clone)]
pub struct ActionSet {
    acts: Vec<Weak<Mutex<dyn Action + Send>>>
}

impl fmt::Debug for ActionSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ActionRegistry")
         .field("acts", &self.acts.iter().fold("".to_string(), |str, a|format!("{}{},",str,a.upgrade().unwrap().lock_it().get_name())))
         .finish()
    }
}

impl ActionSet {
    pub fn create(a: Weak<Mutex<dyn Action + Send>>) -> Self {
        Self {acts: vec![a]}
    }

    pub fn empty() -> Self {
        Self {acts: vec![]}
    }

    pub fn add_action(&mut self, action: Weak<Mutex<dyn Action + Send>>) {
        self.acts.push(action);
    }

    pub async fn call_all(&self, context: &ActionContext) -> Vec<ActionAnswer> {
        let mut res = Vec::new();
        for action in &self.acts {
            match action.upgrade().unwrap().lock_it().call(context).await {
                Ok(a) => res.push(a),
                Err(e) =>  {
                    error!("Action {} failed while being triggered: {}", &action.upgrade().unwrap().lock_it().get_name(), e);
                }
            }
        }
        res
    }
}

// Just a sample action for testing
pub struct SayHelloAction {}

impl SayHelloAction {
    pub fn new() -> Self {
        SayHelloAction{}
    }
}

#[async_trait(?Send)]
impl Action for SayHelloAction {
    async fn call(&self, _context: &ActionContext) -> Result<ActionAnswer> {
        ActionAnswer::send_text("Hello".into(), true)
    }

    fn get_name(&self) -> String {
        "say_hello".into()
    }
}