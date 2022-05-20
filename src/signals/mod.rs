pub mod order;
pub mod poll;
pub mod registries;
pub mod time;

pub use self::order::*;
pub use self::poll::*;
pub use self::registries::*;
pub use self::time::*;

// Standard library
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// This crate
use crate::actions::{Action, ActionAnswer, ActionContext, ActionSet, ContextData, ACT_REG};
use crate::config::Config;
use crate::exts::LockIt;

// Other crates
use anyhow::Result;
use async_trait::async_trait;
use lazy_static::lazy_static;
use unic_langid::LanguageIdentifier;

pub type SignalEventShared = Arc<Mutex<SignalEvent>>;

lazy_static! {
    pub static ref SIG_REG: Mutex<SignalRegistry> = Mutex::new(SignalRegistry::new());
}

#[derive(Debug)]
// A especial signal to be called by the system whenever something happens
pub struct SignalEvent {
    event_map: ActMap,
}

impl SignalEvent {
    pub fn new() -> Self {
        Self {
            event_map: ActMap::new(),
        }
    }

    pub fn add(&mut self, event_name: &str, act_set: ActionSet) {
        self.event_map.add_mapping(event_name, act_set)
    }

    pub async fn call(
        &mut self,
        event_name: &str,
        mut context: ActionContext,
    ) -> Option<Vec<ActionAnswer>> {
        context.data = ContextData::Event {
            event: event_name.to_string(),
        };
        self.event_map.call_mapping(event_name, &context).await
    }
}

#[derive(Debug)]
pub struct ActMap {
    map: HashMap<String, ActionSet>,
}

impl ActMap {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub fn add_mapping(&mut self, order_name: &str, act_set: ActionSet) {
        let action_entry = self
            .map
            .entry(order_name.to_string())
            .or_insert(ActionSet::empty());
        *action_entry = act_set;
    }

    pub async fn call_mapping(
        &mut self,
        act_name: &str,
        context: &ActionContext,
    ) -> Option<Vec<ActionAnswer>> {
        if let Some(action_set) = self.map.get_mut(act_name) {
            Some(action_set.call_all(context).await)
        } else {
            None
        }
    }
}

#[async_trait(?Send)]
pub trait Signal {
    fn end_load(&mut self, curr_lang: &[LanguageIdentifier]) -> Result<()>;
    async fn event_loop(
        &mut self,
        signal_event: SignalEventShared,
        config: &Config,
        curr_lang: &[LanguageIdentifier],
    ) -> Result<()>;
}

#[async_trait(?Send)]
pub trait UserSignal: Signal {
    fn add(
        &mut self,
        data: HashMap<String, String>,
        skill_name: &str,
        act_set: ActionSet,
    ) -> Result<()>;
}

pub struct ActSignal {
    s: Arc<Mutex<dyn UserSignal + Send>>,
    name: String,
}

impl ActSignal {
    pub fn new(s: Arc<Mutex<dyn UserSignal + Send>>, name: String) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self { s, name }))
    }
}

#[async_trait(?Send)]
impl Action for ActSignal {
    async fn call(&mut self, _context: &ActionContext) -> Result<ActionAnswer> {
        // TODO: In theory, Lily should ask which parameters for the signal and
        // which action to be executed but we can't do that right now
        let m = HashMap::new();
        let act_grd = ACT_REG.lock_it();
        let act = act_grd
            .get("embedded", "say_hello")
            .expect("Embedded skill 'say_hello' is not available");

        let acts = ActionSet::create(Arc::downgrade(act));

        self.s.lock_it().add(m, "ActSignal", acts)?;
        ActionAnswer::send_text("Whenever this signals we'll say hello".into(), true)
    }
    fn get_name(&self) -> String {
        self.name.clone()
    }
}
