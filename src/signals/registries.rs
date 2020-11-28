// Standard library
use std::fmt;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use crate::actions::ActionSet;
use crate::config::Config;
use crate::signals::{Signal, SignalEvent, SignalEventShared, SignalRegistryShared};

use anyhow::{anyhow, Result};
use pyo3::{types::PyDict, Py};
use lily_common::extensions::MakeSendable;
use log::warn;
use unic_langid::LanguageIdentifier;

#[derive(Clone)]
pub struct SignalRegistry {
    event: SignalEventShared,
    signals: HashMap<String, Rc<RefCell<dyn Signal>>>
}

impl fmt::Debug for SignalRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SignalRegistry")
            .field("event", &self.event)
            .field("signals", &self.signals.keys().cloned().collect::<Vec<String>>().join(","))
            .finish()
    }
}

impl SignalRegistry {

    pub fn new() -> Self {
        Self {
            event: Arc::new(Mutex::new(SignalEvent::new())),
            signals: HashMap::new()
        }
    }

    pub fn contains(&self, name: &str) -> bool {
        self.signals.contains_key(name)
    }

    pub fn insert(&mut self, sig_name: String, signal: Rc<RefCell<dyn Signal>>) -> Result<()> {
        match self.signals.contains_key(&sig_name) {
            false => {self.signals.insert(sig_name, signal);Ok(())},
            true => Err(anyhow!(format!("Signal {} already exists", sig_name)))
        }
        
    }

    pub fn end_load(&mut self, curr_langs: &Vec<LanguageIdentifier>) -> Result<()> {

        let mut to_remove = Vec::new();

        for (sig_name, signal) in self.signals.iter_mut() {
            if let Err(e) = signal.borrow_mut().end_load(curr_langs) {
                warn!("Signal \"{}\" had trouble in \"end_load\", will be disabled, error: {}", &sig_name, e);

                to_remove.push(sig_name.to_owned());
            }
        }

        // Delete any signals which had problems during end_load
        for sig_name in &to_remove {
            self.signals.remove(sig_name);
        }

        Ok(())
    }

    pub async fn call_loop(&mut self,
        sig_name: &str,
        config: &Config,
        base_context: &Py<PyDict>,
        curr_lang: &Vec<LanguageIdentifier>
    ) -> Result<()> {
        self.signals[sig_name].borrow_mut().event_loop(self.event.clone(), config, base_context, curr_lang).await
    }

    pub fn get_map_ref(&self) -> &HashMap<String,Rc<RefCell<dyn Signal>>> {
        &self.signals
    }
}

// To show each package just those signals available to them
#[derive(Clone)]
pub struct LocalSignalRegistry {
    event: SignalEventShared,
    signals: HashMap<String, Rc<RefCell<dyn Signal>>>,
    global_reg: SignalRegistryShared
}

impl fmt::Debug for LocalSignalRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalSignalRegistry")
            .field("event", &self.event)
            .field("signals", &self.signals.keys().cloned().collect::<Vec<String>>().join(","))
            .field("global_reg", &self.global_reg)
            .finish()
    }
}

impl LocalSignalRegistry {
    pub fn new(global_reg: SignalRegistryShared) -> Self {
        Self {
            event: {global_reg.borrow().event.clone()},
            signals: HashMap::new(),
            global_reg: {global_reg.clone()}
        }
    }

    pub fn add_sigact_rel(&mut self,sig_name: &str, sig_arg: serde_yaml::Value, skill_name: &str, pkg_name: &str, act_set: Arc<Mutex<ActionSet>>) -> Result<()> {
        if sig_name == "event" {
            self.event.lock().sendable()?.add(skill_name, act_set);
            Ok(())
        }
        else {
            match self.signals.get(sig_name) {
                Some(signal) => signal.borrow_mut().add(sig_arg, skill_name, pkg_name, act_set),
                None => Err(anyhow!("Signal named \"{}\" was not found", sig_name))
            }
        }
    }

    pub fn insert(&mut self, sig_name: String, signal: Rc<RefCell<dyn Signal>>) -> Result<()> {
        (*self.global_reg).borrow_mut().insert(sig_name.clone(), signal.clone())?;
        self.signals.insert(sig_name, signal);

        Ok(())
    }

    pub fn extend(&mut self, other: Self) {
        self.signals.extend(other.signals);
    }

    pub fn extend_with_map(&mut self, other: HashMap<String, Rc<RefCell<dyn Signal>>>) {
        self.signals.extend(other);
    }

    pub fn minus(&self, other: &Self) -> Self {
        let mut res = Self{
            event: self.event.clone(),
            signals: HashMap::new(),
            global_reg: self.global_reg.clone()
        };

        for (k,v) in &self.signals {
            if !other.signals.contains_key(k) {
                res.signals.insert(k.clone(), v.clone());
            }
        }

        res
    }

    pub fn remove_from_global(&self) {
        for (sgnl,_) in &self.signals {
            self.global_reg.borrow_mut().signals.remove(sgnl);
        }
    }

    pub fn get_global_mut(&self) -> std::cell::RefMut<SignalRegistry> {
        (*self.global_reg).borrow_mut()
    }
}

