// Standard library
use std::cell::{RefCell, RefMut};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use crate::actions::ActionContext;
use crate::config::Config;
use crate::collections::{BaseRegistry, GlobalReg, LocalBaseRegistry};
use crate::signals::{Signal, SignalEvent, SignalEventShared, SignalOrderCurrent, SignalRegistryShared};

use anyhow::Result;
use delegate::delegate;
use log::{error, warn};
use tokio::task::LocalSet;
use unic_langid::LanguageIdentifier;

#[derive(Debug, Clone)]
pub struct SignalRegistry {
    event: SignalEventShared,
    order: Option<Rc<RefCell<SignalOrderCurrent>>>,
    base: BaseRegistry<dyn Signal>
}

impl SignalRegistry {

    pub fn new() -> Self {
        Self {
            event: Arc::new(Mutex::new(SignalEvent::new())),
            order: None,
            base: BaseRegistry::new()
        }
    }

    pub fn end_load(&mut self, curr_langs: &Vec<LanguageIdentifier>) -> Result<()> {

        let mut to_remove = Vec::new();

        for (sig_name, signal) in self.base.iter_mut() {
            if let Err(e) = signal.borrow_mut().end_load(curr_langs) {
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
        for (sig_name, sig) in self.base.clone() {
            let event = self.event.clone();
            let config = config.clone();
            let base_context = base_context.clone();
            let curr_lang = curr_lang.clone();
            local.spawn_local(async move {

                let res = sig.borrow_mut().event_loop(event, &config, &base_context, &curr_lang).await;
                if let Err(e) = res {
                    error!("Signal '{}' had an error: {}", sig_name, e.to_string());
                }
            });
        }

        local.await;
        Ok(())
        
    }

    pub fn set_order(&mut self, sig_order: Rc<RefCell<SignalOrderCurrent>>) -> Result<()>{
        self.order = Some(sig_order.clone());
        self.insert("order".to_string(), sig_order)
    }

    delegate!{to self.base{
        pub fn contains(&self, name: &str) -> bool;
        pub fn get_map_ref(&mut self) -> &HashMap<String,Rc<RefCell<dyn Signal>>>;
    }}
}

impl GlobalReg<dyn Signal> for SignalRegistry {
    fn remove(&mut self, sig_name: &str) {
        self.base.remove(sig_name)
    }

    fn insert(&mut self, sig_name: String, signal: Rc<RefCell<dyn Signal>>) -> Result<()> {
        self.base.insert(sig_name, signal)
    }
}

// To show each skill just those signals available to them
#[derive(Debug, Clone)]
pub struct LocalSignalRegistry {
    event: SignalEventShared,
    base: LocalBaseRegistry<dyn Signal, SignalRegistry>
}


impl LocalSignalRegistry {
    
    pub fn new(global_reg: SignalRegistryShared) -> Self {
        Self {
            event: {global_reg.borrow().event.clone()},
            base: LocalBaseRegistry::new(global_reg.clone())
        }
    }

    pub fn extend(&mut self, other: LocalSignalRegistry) {
        self.base.extend(other.base);
    }

    pub fn minus(&self, other: &Self) -> Self{
        let new_base = self.base.minus(&other.base);
        Self { 
            event: {self.event.clone()},
            base: new_base
        }
    }

    
    pub fn get_sig_order(&self) -> Option<Rc<RefCell<SignalOrderCurrent>>> {
        self.get_global_mut().order.clone()
    }

    pub fn get_sig_event(&self) -> Arc<Mutex<SignalEvent>> {
        self.event.clone()
    }

    // Just reuse methods
    delegate! {to self.base{
        pub fn insert(&mut self, sig_name: String, signal: Rc<RefCell<dyn Signal>>) -> Result<()>;
        pub fn extend_with_map(&mut self, other: HashMap<String, Rc<RefCell<dyn Signal>>>);
        pub fn remove_from_global(&self);
        pub fn get(&self, action_name: &str) -> Option<&Rc<RefCell<dyn Signal>>>;
        pub fn get_global_mut(&self) -> RefMut<SignalRegistry>;
    }}
}

