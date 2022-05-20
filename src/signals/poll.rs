use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::actions::{ActionContext, ActionSet, ContextData};
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
        Self {
            query,
            act_set,
            condition: Condition::Changed(RefCell::new(Vec::new())),
        }
    }
}

#[derive(Debug)]
pub struct PollQuery {
    tasks: Vec<UserTask>,
}

impl PollQuery {
    pub fn new() -> Self {
        Self { tasks: Vec::new() }
    }
}

#[async_trait(?Send)]
impl Signal for PollQuery {
    fn end_load(&mut self, _curr_lang: &[LanguageIdentifier]) -> Result<()> {
        Ok(())
    }
    async fn event_loop(
        &mut self,
        _signal_event: SignalEventShared,
        _config: &Config,
        curr_lang: &[LanguageIdentifier],
    ) -> Result<()> {
        loop {
            sleep(Duration::from_secs(30)).await;
            for task in &mut self.tasks {
                if task.condition.check(&task.query, HashMap::new()) {
                    let context = ActionContext {
                        locale: curr_lang[0].to_string(),
                        satellite: None,
                        data: ContextData::Event {
                            event: "called by user signal".into(),
                        },
                    };
                    task.act_set.call_all(&context).await;
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
