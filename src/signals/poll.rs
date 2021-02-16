use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::actions::{ActionContext, ActionSet};
use crate::config::Config;
use crate::queries::{Condition, Query};
use crate::signals::{Signal, SignalEventShared};
use crate::vars::POISON_MSG;

use anyhow::Result;
use async_trait::async_trait;
use tokio::time::sleep;
use unic_langid::LanguageIdentifier;

struct UserTask {
    query: Rc<RefCell<dyn Query>>,
    condition: Condition,
    act_set: Arc<Mutex<ActionSet>>,
}

impl UserTask {
    fn new(query: Rc<RefCell<dyn Query>>, act_set: Arc<Mutex<ActionSet>>) -> Self {
        Self {query, act_set, condition: Condition::Test}
    }
}

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
    async fn event_loop(&mut self, _signal_event: SignalEventShared, _config: &Config, base_context: &ActionContext, _curr_lang: &Vec<LanguageIdentifier>) -> Result<()> {
        loop {
            sleep(Duration::from_secs(30)).await;
            for task in &self.tasks {
                if task.condition.check(&task.query) {
                    task.act_set.lock().expect(POISON_MSG).call_all(base_context);
                }
            }
        }
    }
}

impl PollQuery {
    fn add(&mut self, query: Rc<RefCell<dyn Query>>, act_set: Arc<Mutex<ActionSet>>) -> Result<()> {
        let task = UserTask::new(query, act_set);
        self.tasks.push(task);

        Ok(())
    }
}