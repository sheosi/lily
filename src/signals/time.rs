use std::fmt;
use std::time::{Duration};
use std::sync::{Arc, Mutex};

use crate::actions::{ActionContext, ActionSet, SharedActionSet};
use crate::config::Config;
use crate::signals::{Signal, SignalEventShared};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use tokio::{spawn, time::sleep};
use serde::{de::{self, Visitor}, Deserialize, Deserializer};
use unic_langid::LanguageIdentifier;

pub struct Timer {
    timers: Vec<(TimerKind, Arc<Mutex<ActionSet>>)>,
}

#[derive(Clone, Debug)]
struct MyDateTime {
    inner: DateTime<Utc>,
}

struct MyDateTimeVisitor;

impl<'de> Visitor<'de> for MyDateTimeVisitor {
    type Value = MyDateTime;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a string with format \"yyyy-mm-dd hh:mm:ss")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value,E>
    where E: de::Error {
        let naive = NaiveDateTime::parse_from_str(v, "%F %T")
                                .map_err(|_|E::custom(
                                    format!("date '{}' is not correctly formatted",v)
                                ))?;
        let inner = DateTime::<Utc>::from_utc(naive, Utc);
        Ok(MyDateTime{inner})
    }
}
impl<'de> Deserialize<'de> for MyDateTime {
    fn deserialize<D>(deserializer: D) -> Result<MyDateTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_i32(MyDateTimeVisitor)
    }
}
#[derive(Clone, Debug, Deserialize)]
enum TimerKind {
    Once(Duration),
    Every(Duration),
    On(MyDateTime)
}

#[async_trait(?Send)]
impl Signal for Timer {
    fn add(&mut self, sig_arg: serde_yaml::Value, _skill_name: &str, _pkg_name: &str, act_set: Arc<Mutex<ActionSet>>) -> Result<()> {
        match serde_yaml::from_value(sig_arg) {
            Ok(a) => {
                self.timers.push((a, act_set));
                Ok(())
            }
            Err(_) => Err(anyhow!("Timer argument wasn't ok"))
        }
    
    }
    fn end_load(&mut self, _curr_lang: &Vec<LanguageIdentifier>) -> Result<()> {
        Ok(())
    }
    async fn event_loop(&mut self, _signal_event: SignalEventShared, _config: &Config, base_context: &ActionContext, _curr_lang: &Vec<LanguageIdentifier>) -> Result<()> {
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
                        let dur = date.inner.signed_duration_since(Utc::now()).to_std().unwrap();
                        sleep(dur).await;
                        actions.call_all(&base_context, ||{format!("timer on date: {:?}", date)});
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