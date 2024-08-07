use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

use crate::actions::{ActionContext, ActionSet, ContextData};
use crate::config::Config;
use crate::signals::{Signal, SignalEventShared, UserSignal};
use crate::vars::UNEXPECTED_MSG;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use log::warn;
use serde::{
    de::{self, Visitor},
    Deserialize, Deserializer,
};
use tokio::{task::spawn_local, time::sleep};
use unic_langid::LanguageIdentifier;

pub struct Timer {
    timers: Vec<(TimerKind, ActionSet)>,
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

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let naive = NaiveDateTime::parse_from_str(v, "%F %T")
            .map_err(|_| E::custom(format!("date '{}' is not correctly formatted", v)))?;
        let inner = DateTime::<Utc>::from_utc(naive, Utc);
        Ok(MyDateTime { inner })
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
impl MyDateTime {
    fn parse(date_str: &str) -> Result<Self> {
        let naive = NaiveDateTime::parse_from_str(date_str, "%Y-%m-%d %H:%M:%S")?;
        let date = DateTime::<Utc>::from_utc(naive, Utc);
        Ok(Self { inner: date })
    }
}
#[derive(Clone, Debug, Deserialize)]
enum TimerKind {
    Once(Duration),
    Every(Duration),
    On(MyDateTime),
}

#[async_trait(?Send)]
impl Signal for Timer {
    fn end_load(&mut self, _curr_lang: &[LanguageIdentifier]) -> Result<()> {
        Ok(())
    }
    async fn event_loop(
        &mut self,
        _signal_event: SignalEventShared,
        _config: &Config,
        curr_lang: &[LanguageIdentifier],
    ) -> Result<()> {
        fn make_context(lang: &LanguageIdentifier) -> ActionContext {
            ActionContext {
                locale: lang.to_string(),
                satellite: None,
                data: ContextData::Event {
                    event: "timer".to_string(),
                },
            }
        }
        for (timer, actions) in &self.timers {
            let timer = timer.clone();
            let actions = actions.clone();

            match timer {
                TimerKind::Once(dur) => {
                    let def_lang = curr_lang[0].clone();
                    spawn_local(async move {
                        sleep(dur).await;
                        actions.call_all(&make_context(&def_lang)).await;
                    });
                }
                TimerKind::Every(dur) => {
                    let def_lang = curr_lang[0].clone();
                    spawn_local(async move {
                        loop {
                            sleep(dur).await;
                            actions.call_all(&make_context(&def_lang)).await;
                        }
                    });
                }
                TimerKind::On(date) => {
                    let def_lang = curr_lang[0].clone();
                    spawn_local(async move {
                        let dur = date
                            .inner
                            .signed_duration_since(Utc::now())
                            .to_std()
                            .expect(UNEXPECTED_MSG);
                        sleep(dur).await;
                        actions.call_all(&make_context(&def_lang)).await;
                    });
                }
            }
        }
        Ok(())
    }
}
#[async_trait(?Send)]
impl UserSignal for Timer {
    fn add(
        &mut self,
        data: HashMap<String, String>,
        _skill_name: &str,
        act_set: ActionSet,
    ) -> Result<()> {
        self.timers.push((Self::from_data(data)?, act_set));
        Ok(())
    }
}

impl Timer {
    pub fn new() -> Self {
        Self { timers: Vec::new() }
    }

    fn from_data(data: HashMap<String, String>) -> Result<TimerKind> {
        if data.contains_key("seconds")
            || data.contains_key("minutes")
            || data.contains_key("hours")
        {
            fn get_time(
                data: &HashMap<String, String>,
                name: &str,
            ) -> Result<u64, std::num::ParseIntError> {
                data.get(name)
                    .map(|s| {
                        let res = s.parse();
                        if res.is_err() {
                            warn!("'{}' can't be parsed as number", s);
                        }
                        res
                    })
                    // If there's no record just return 0
                    .unwrap_or(Ok(0))
            }
            let secs = get_time(&data, "seconds")?;
            let mins = get_time(&data, "minutes")?;
            let hours = get_time(&data, "hours")?;

            let dur = Duration::from_secs(secs + mins * 60 + hours * 3600);

            if data.contains_key("kind") && data["kind"] == "every" {
                Ok(TimerKind::Every(dur))
            } else {
                Ok(TimerKind::Once(dur))
            }
        } else if data.contains_key("date") {
            Ok(TimerKind::On(MyDateTime::parse(&data["date"])?))
        } else {
            Err(anyhow!("Non-coincident format for timer"))
        }
    }
}
