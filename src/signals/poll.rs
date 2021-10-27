use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::actions::{DynamicDict, ActionSet};
use crate::config::Config;
use crate::queries::{Condition, Query};
use crate::signals::{Signal, SignalEventShared};

use anyhow::Result;
use async_trait::async_trait;
use tokio::time::sleep;
use unic_langid::LanguageIdentifier;

impl std::fmt::Debug for UserTask {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("SnipsNlu").finish()
    }
}

struct UserTask {
    query: Arc<Mutex<dyn Query + Send>>,
    condition: Condition,
    act_set: ActionSet,
}

impl UserTask {
    fn new(query: Arc<Mutex<dyn Query + Send>>, act_set: ActionSet) -> Self {
        Self {query, act_set, condition: Condition::Changed(RefCell::new(Vec::new()))}
    }
}

#[derive(Debug)]
pub struct PollQuery {
    tasks: Vec<UserTask>
}
        
impl PollQuery {
    pub fn new() -> Self {
        Self{tasks: Vec::new()}
    }
}

#[async_trait(?Send)]
impl Signal for PollQuery {
    fn end_load(&mut self, _curr_lang: &Vec<LanguageIdentifier>) -> Result<()> {
        Ok(())
    }
    async fn event_loop(&mut self, _signal_event: SignalEventShared, _config: &Config, base_context: &DynamicDict, _curr_lang: &Vec<LanguageIdentifier>) -> Result<()> {
        loop {
            sleep(Duration::from_secs(30)).await;
            for task in &mut self.tasks {
                if task.condition.check(&task.query, HashMap::new()) {
                    task.act_set.call_all(base_context).await;
                }
            }
        }
    }
}

impl PollQuery {
    pub fn add(&mut self, query: Arc<Mutex<dyn Query + Send>>, act_set: ActionSet) -> Result<()> {
        let task = UserTask::new(query, act_set);
        self.tasks.push(task);

        Ok(())
    }
}