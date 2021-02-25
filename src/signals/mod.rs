pub mod order;
pub mod poll;
pub mod python_sigs;
pub mod registries;
pub mod time;

pub use self::order::*;
pub use self::poll::*;
pub use self::python_sigs::*;
pub use self::registries::*;
pub use self::time::*;

// Standard library
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

// This crate
use crate::actions::{ActionAnswer, ActionContext, ActionSet, SharedActionSet};
use crate::config::Config;

// Other crates
use anyhow::Result;
use async_trait::async_trait;
use unic_langid::LanguageIdentifier;

pub type SignalEventShared = Arc<Mutex<SignalEvent>>;
pub type SignalRegistryShared = Rc<RefCell<SignalRegistry>>;

#[derive(Debug)]
// A especial signal to be called by the system whenever something happens
pub struct SignalEvent {
    event_map: ActMap
}

impl SignalEvent {
    pub fn new() -> Self {
        Self {event_map: ActMap::new()}
    }

    pub fn add(&mut self, event_name: &str, act_set: Arc<Mutex<ActionSet>>) {
        self.event_map.add_mapping(event_name, act_set)
    }

    pub fn call(&mut self, event_name: &str, mut context: ActionContext) -> Option<Vec<ActionAnswer>> {
        context.set("type".to_string(), "event".to_string());
        context.set("event".to_string(), "event_name".to_string());
        self.event_map.call_mapping(event_name, &context)
    }
}

#[derive(Debug)]
pub struct ActMap {
    map: HashMap<String, Arc<Mutex<ActionSet>>>
}

impl ActMap {
    pub fn new() -> Self {
        Self{map: HashMap::new()}
    }

    pub fn add_mapping(&mut self, order_name: &str, act_set: Arc<Mutex<ActionSet>>) {
        let action_entry = self.map.entry(order_name.to_string()).or_insert(ActionSet::create());
        *action_entry = act_set;
    }

    pub fn call_mapping(&mut self, act_name: &str, context: &ActionContext) -> Option<Vec<ActionAnswer>>{
        if let Some(action_set) = self.map.get_mut(act_name) {
            Some(action_set.call_all(context, ||{act_name.into()}))
        }
        else {
            None
        }
    }
}

#[async_trait(?Send)]
pub trait Signal {
    fn end_load(&mut self, curr_lang: &Vec<LanguageIdentifier>) -> Result<()>;
    async fn event_loop(&mut self, signal_event: SignalEventShared, config: &Config, base_context: &ActionContext, curr_lang: &Vec<LanguageIdentifier>) -> Result<()>;
}

#[async_trait(?Send)]
pub trait UserSignal {
    fn add(&mut self, data: HashMap<String, String>, intent_name: &str, skill_name: &str, act_set: Arc<Mutex<ActionSet>>) -> Result<()>;
}


