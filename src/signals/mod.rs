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
use crate::actions::{ActionSet, ActionContext, SharedActionSet};
use crate::config::Config;

// Other crates
use anyhow::Result;
use async_trait::async_trait;
use lily_common::extensions::MakeSendable;
use unic_langid::LanguageIdentifier;

pub type SignalEventShared = Arc<Mutex<SignalEvent>>;
pub type SignalRegistryShared = Rc<RefCell<SignalRegistry>>;

#[derive(Debug)]
// A especial signal to be called by the system whenever something happens
pub struct SignalEvent {
    event_map: OrderMap
}

impl SignalEvent {
    pub fn new() -> Self {
        Self {event_map: OrderMap::new()}
    }

    pub fn add(&mut self, event_name: &str, act_set: Arc<Mutex<ActionSet>>) {
        self.event_map.add_order(event_name, act_set)
    }

    pub fn call(&mut self, event_name: &str, context: &ActionContext) {
        self.event_map.call_order(event_name, context)
    }
}

#[derive(Debug)]
pub struct OrderMap {
    map: HashMap<String, Arc<Mutex<ActionSet>>>
}

impl OrderMap {
    pub fn new() -> Self {
        Self{map: HashMap::new()}
    }

    pub fn add_order(&mut self, order_name: &str, act_set: Arc<Mutex<ActionSet>>) {
        let action_entry = self.map.entry(order_name.to_string()).or_insert(ActionSet::create());
        *action_entry = act_set;
    }

    pub fn call_order(&mut self, act_name: &str, context: &ActionContext) {
        if let Some(action_set) = self.map.get_mut(act_name) {
            action_set.call_all(context, ||{act_name.into()});
        }
    }
}

#[async_trait(?Send)]
pub trait Signal {
    fn add(&mut self, sig_arg: serde_yaml::Value, skill_name: &str, pkg_name: &str, act_set: Arc<Mutex<ActionSet>>) -> Result<()>;
    fn end_load(&mut self, curr_lang: &Vec<LanguageIdentifier>) -> Result<()>;
    async fn event_loop(&mut self, signal_event: SignalEventShared, config: &Config, base_context: &ActionContext, curr_lang: &Vec<LanguageIdentifier>) -> Result<()>;
}


