use std::time::{Duration};
use std::sync::{Arc, Mutex};

use crate::actions::{ActionContext, ActionSet, SharedActionSet};
use crate::config::Config;
use crate::signals::{Signal, SignalEventShared};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::{spawn, time::sleep};
use serde::Deserialize;
use unic_langid::LanguageIdentifier;

pub struct Timer {
    timers: Vec<(TimerKind, Arc<Mutex<ActionSet>>)>,
}

#[derive(Clone, Debug, Deserialize)]
enum TimerKind {
    Once(Duration),
    Every(Duration),
    On(DateTime<Utc>)
}

#[async_trait(?Send)]
impl Signal for Timer {
    fn add(&mut self, sig_arg: serde_yaml::Value, skill_name: &str, pkg_name: &str, act_set: Arc<Mutex<ActionSet>>) -> Result<()> {
        match serde_yaml::from_value(sig_arg) {
            Ok(a) => {
                self.timers.push((a, act_set));
                Ok(())
            }
            Err(e) => Err(anyhow!("Timer argument wasn't ok"))
        }
    
    }
    fn end_load(&mut self, curr_lang: &Vec<LanguageIdentifier>) -> Result<()> {
        Ok(())
    }
    async fn event_loop(&mut self, signal_event: SignalEventShared, config: &Config, base_context: &ActionContext, curr_lang: &Vec<LanguageIdentifier>) -> Result<()> {
        for (timer, actions) in &self.timers {
            let base_context = base_context.clone();
            let timer = timer.clone();
            let actions = actions.clone();

            match timer {
                TimerKind::Once(dur) => {
                    spawn(async move {
                        sleep(dur).await;
                        actions.call_all(&base_context, ||{format!("{:?}", timer)});
                    });
                },
                TimerKind::Every(dur) => {
                    spawn(async move {
                        loop {
                            sleep(dur).await;
                            actions.call_all(&base_context, ||{format!("{:?}", timer)});
                        }
                    });
                },
                TimerKind::On(date) => {
                    spawn( async move {
                        let dur = date.signed_duration_since(Utc::now()).to_std().unwrap();
                        sleep(dur).await;
                        actions.call_all(&base_context, ||{format!("{:?}", timer)});
                    });
                }
            }
        }
        Ok(())
    }
}

impl Timer {
    pub fn new() -> Self {
        Self {timers: Vec::new()}
    }
}