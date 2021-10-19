// Standard library
use std::mem::replace;
use std::sync::{Arc, Mutex};

// This crate
use crate::actions::{ACT_REG, SayHelloAction};
use crate::exts::LockIt;
use crate::signals::dynamic_nlu::DynamicNluRequest;
use crate::signals::{SIG_REG,  new_signal_order, poll::PollQuery, Timer};
use crate::skills::SkillLoader;

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

impl SkillLoader for EmbeddedLoader {
    fn load_skills(&mut self, 
        langs: &Vec<LanguageIdentifier>) -> Result<()> {

        let consumer = replace(&mut self.consumer, None).expect("Consumer already consumed");
        
        {
            let mut mut_sigreg = SIG_REG.lock_it();
            mut_sigreg.set_order(Arc::new(Mutex::new(new_signal_order(langs.to_owned(), consumer))))?;
            mut_sigreg.set_poll(Arc::new(Mutex::new(PollQuery::new())))?;
            mut_sigreg.insert("embedded".into(),"timer".into(), Arc::new(Mutex::new(Timer::new())))?;
        }

        {
            let mut mut_actreg = ACT_REG.lock_it();        
            mut_actreg.insert("embedded".into(),"say_hello".into(), Arc::new(Mutex::new(SayHelloAction::new())))?;
        }
        

        Ok(())
    }
}