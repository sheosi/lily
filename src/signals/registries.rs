// Standard library
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::actions::{Action, ActionContext, LocalActionRegistry};
use crate::exts::LockIt;
use crate::config::Config;
use crate::collections::{BaseRegistrySend, GlobalRegSend, LocalBaseRegistrySend};
use crate::queries::LocalQueryRegistry;
use crate::signals::poll::PollQuery;
use crate::signals::{Signal, SignalEvent, SignalEventShared, SignalOrderCurrent, SignalRegistryShared, UserSignal};

use anyhow::{anyhow, Result};
use delegate::delegate;
use lazy_static::lazy_static;
use log::{error, warn};
use tokio::task::LocalSet;
use unic_langid::LanguageIdentifier;

lazy_static!{
    pub static ref POLL_SIGNAL: Mutex<Option<Arc<Mutex<PollQuery>>>> = Mutex::new(None);
    pub static ref QUERY_REG: Mutex<HashMap<String, LocalQueryRegistry>> = Mutex::new(HashMap::new());
    pub static ref ACT_REG: Mutex<HashMap<String, LocalActionRegistry>> = Mutex::new(HashMap::new());
}

#[derive(Debug, Clone)]
pub struct SignalRegistry {
    event: SignalEventShared,
    order: Option<Arc<Mutex<SignalOrderCurrent>>>,
    poll: Option<Arc<Mutex<PollQuery>>>,
    base: BaseRegistrySend<dyn UserSignal + Send>
}

impl SignalRegistry {

    pub fn new() -> Self {
        Self {
            event: Arc::new(Mutex::new(SignalEvent::new())),
            order: None,
            poll: None,
            base: BaseRegistrySend::new()
        }
    }

    pub fn end_load(&mut self, curr_langs: &Vec<LanguageIdentifier>) -> Result<()> {

        let mut to_remove = Vec::new();

        for (sig_name, signal) in self.base.iter_mut() {
            if let Err(e) = signal.lock_it().end_load(curr_langs) {
                warn!("Signal \"{}\" had trouble in \"end_load\", will be disabled, error: {}", &sig_name, e);

                to_remove.push(sig_name.to_owned());
            }
        }

        // Delete any signals which had problems during end_load
        for sig_name in &to_remove {
            self.base.remove(sig_name);
        }

        Ok(())
    }

    pub async fn call_loops(&mut self,
        config: &Config,
        base_context: &ActionContext,
        curr_lang: &Vec<LanguageIdentifier>
    ) -> Result<()> {
        let local = LocalSet::new();

        let spawn_on_local = |n: String, s:Arc<Mutex<dyn Signal + Send>>| {            
            let event = self.event.clone();
            let config = config.clone();
            let base_context = base_context.clone();
            let curr_lang = curr_lang.clone();

            local.spawn_local(async move {

                let res = s.lock_it().event_loop(event, &config, &base_context, &curr_lang).await;
                if let Err(e) = res {
                    error!("Signal '{}' had an error: {}", n, e.to_string());
                }
            });
        };

        let spawn_on_local_u = |n: String, s:Arc<Mutex<dyn UserSignal + Send>>| {
            let event = self.event.clone();
            let config = config.clone();
            let base_context = base_context.clone();
            let curr_lang = curr_lang.clone();
            
            local.spawn_local(async move {

                let res = s.lock_it().event_loop(event, &config, &base_context, &curr_lang).await;
                if let Err(e) = res {
                    error!("Signal '{}' had an error: {}", n, e.to_string());
                }
            });
        };

        spawn_on_local("order".into(), self.order.as_ref().expect("Order signal had problems during init").clone());
        spawn_on_local("poll".into(), self.poll.as_ref().expect("Poll signal had problems during init").clone());
        for (sig_name, sig) in self.base.clone() {
            spawn_on_local_u(sig_name, sig);
        }

        local.await;
        Ok(())
        
    }

    pub fn set_order(&mut self, sig_order: Arc<Mutex<SignalOrderCurrent>>) -> Result<()>{
        self.order = Some(sig_order);
        Ok(())
    }

    pub fn set_poll(&mut self, sig_poll: Arc<Mutex<PollQuery>>) -> Result<()>{
        *POLL_SIGNAL.lock_it() = Some(sig_poll.clone());
        self.poll = Some(sig_poll);
        Ok(())
    }

    delegate!{to self.base{
        pub fn get_map_ref(&mut self) -> &HashMap<String,Arc<Mutex<dyn UserSignal + Send>>>;
    }}
}

impl GlobalRegSend<dyn UserSignal + Send> for SignalRegistry {
    fn remove(&mut self, sig_name: &str) {
        self.base.remove(sig_name)
    }

    delegate!{to self.base{
        fn insert(&mut self, skill_name: String, sig_name: String, signal: Arc<Mutex<dyn UserSignal + Send>>) -> Result<()>;
    }}
    
}

// To show each skill just those signals available to them
#[derive(Debug, Clone)]
pub struct LocalSignalRegistry {
    event: SignalEventShared,
    base: LocalBaseRegistrySend<dyn UserSignal + Send, SignalRegistry>
}


impl LocalSignalRegistry {
    
    pub fn new(global_reg: SignalRegistryShared) -> Self {
        Self {
            event: {global_reg.lock_it().event.clone()},
            base: LocalBaseRegistrySend::new(global_reg.clone())
        }
    }

    pub fn minus(&self, other: &Self) -> Self{
        let new_base = self.base.minus(&other.base);
        Self { 
            event: {self.event.clone()},
            base: new_base
        }
    }

    
    pub fn get_sig_order(&self) -> Option<Arc<Mutex<SignalOrderCurrent>>> {
        self.get_global_mut().order.clone()
    }

    pub fn get_sig_event(&self) -> Arc<Mutex<SignalEvent>> {
        self.event.clone()
    }

    // Just reuse methods
    delegate! {to self.base{
        pub fn extend_with_map(&mut self, other: HashMap<String, Arc<Mutex<dyn UserSignal + Send>>>);
        pub fn remove_from_global(&self);
        pub fn get(&self, action_name: &str) -> Option<&Arc<Mutex<dyn UserSignal + Send>>>;
        pub fn get_global_mut(&self) -> MutexGuard<SignalRegistry>;
    }}
}

pub fn dynamically_add_action(skill_name: String, action_name: &str, action: Arc<Mutex<dyn Action + Send>>) -> Result<()> {
    let act_reg_mutex = ACT_REG.lock_it();
    let local_skill = act_reg_mutex.get(&skill_name).ok_or_else(||anyhow!("Skill does not exist"))?;
    local_skill.get_global_mut().insert(skill_name, action_name.to_owned(), action)?;
    //TODO! What should we do with the local action itself?

    Ok(())
}
