// Standard library
use std::mem::replace;
use std::sync::{Arc, Mutex};

// This crate
use crate::actions::{LocalActionRegistry, SayHelloAction};
use crate::collections::GlobalRegSend;
use crate::queries::{LocalQueryRegistry};
use crate::signals::dynamic_nlu::DynamicNluRequest;
use crate::signals::{ LocalSignalRegistry, new_signal_order, poll::PollQuery, Timer};
use crate::skills::Loader;

// Other crates
use anyhow::Result;
use tokio::sync::mpsc::Receiver;
use unic_langid::LanguageIdentifier;
pub struct EmbeddedLoader {
    consumer: Option<Receiver<DynamicNluRequest>>
}

impl EmbeddedLoader {
    pub fn new(consumer: Receiver<DynamicNluRequest>) -> Self {
        Self{consumer: Some(consumer)}
    }
}

impl Loader for EmbeddedLoader {
    fn load_skills(&mut self, 
        base_sigreg: &LocalSignalRegistry,
        base_actreg: &LocalActionRegistry,
        _base_queryreg: &LocalQueryRegistry,
        langs: &Vec<LanguageIdentifier>) -> Result<()> {
        let mut mut_sigreg = base_sigreg.get_global_mut();
        let mut mut_actreg = base_actreg.get_global_mut();
        let consumer = replace(&mut self.consumer, None).expect("Consumer already consumed");
        
        mut_sigreg.set_order(Arc::new(Mutex::new(new_signal_order(langs.to_owned(), consumer))))?;
        mut_sigreg.set_poll(Arc::new(Mutex::new(PollQuery::new())))?;
        mut_sigreg.insert("embedded".into(),"timer".into(), Arc::new(Mutex::new(Timer::new())))?;
        mut_actreg.insert("embedded".into(),"say_hello".into(), Arc::new(Mutex::new(SayHelloAction::new())))?;

        Ok(())
    }
}