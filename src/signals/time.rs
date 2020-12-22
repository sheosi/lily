use std::sync::{Arc, Mutex};

use crate::actions::{ActionContext, ActionSet};
use crate::config::Config;
use crate::signals::{Signal, SignalEventShared};

use anyhow::Result;
use async_trait::async_trait;
use unic_langid::LanguageIdentifier;

pub struct Timer {}

#[async_trait(?Send)]
impl Signal for Timer {
    fn add(&mut self, sig_arg: serde_yaml::Value, skill_name: &str, pkg_name: &str, act_set: Arc<Mutex<ActionSet>>) -> Result<()> {
        Ok(())
    }
    fn end_load(&mut self, curr_lang: &Vec<LanguageIdentifier>) -> Result<()> {
        Ok(())
    }
    async fn event_loop(&mut self, signal_event: SignalEventShared, config: &Config, base_context: &ActionContext, curr_lang: &Vec<LanguageIdentifier>) -> Result<()> {
        Ok(())
    }
}

impl Timer {
    pub fn new() -> Self {
        Self {}
    }
}