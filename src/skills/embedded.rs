// Standard library
use std::sync::{Arc, Mutex};

// This crate
use crate::actions::{SayHelloAction, ACT_REG};
use crate::exts::LockIt;
use crate::signals::{new_signal_order, poll::PollQuery, Timer, SIG_REG};
use crate::skills::SkillLoader;

// Other crates
use anyhow::Result;
use async_trait::async_trait;
use unic_langid::LanguageIdentifier;
pub struct EmbeddedLoader {}

impl EmbeddedLoader {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait(?Send)]
impl SkillLoader for EmbeddedLoader {
    fn load_skills(&mut self, langs: &[LanguageIdentifier]) -> Result<()> {
        {
            let mut mut_sigreg = SIG_REG.lock_it();
            mut_sigreg.set_order(Arc::new(Mutex::new(new_signal_order(langs.to_owned()))))?;
            mut_sigreg.set_poll(Arc::new(Mutex::new(PollQuery::new())))?;
            mut_sigreg.insert(
                "embedded",
                "timer",
                Arc::new(Mutex::new(Timer::new())),
            )?;
        }

        {
            let mut mut_actreg = ACT_REG.lock_it();
            mut_actreg.insert(
                "embedded",
                "say_hello",
                Arc::new(Mutex::new(SayHelloAction::new())),
            )?;
        }

        Ok(())
    }

    async fn run_loader(&mut self) -> Result<()> {
        Ok(())
    }
}
