// Standard library
use std::sync::{Arc, Mutex};

use crate::collections::BaseRegistry;
use crate::config::Config;
use crate::exts::LockIt;
use crate::signals::poll::PollQuery;
use crate::signals::{Signal, SignalEvent, SignalEventShared, SignalOrderCurrent, UserSignal};

use anyhow::Result;
use delegate::delegate;
use lazy_static::lazy_static;
use log::{error, warn};
use tokio::task::LocalSet;
use unic_langid::LanguageIdentifier;

lazy_static! {
    pub static ref POLL_SIGNAL: Mutex<Option<Arc<Mutex<PollQuery>>>> = Mutex::new(None);
}

#[derive(Debug, Clone)]
pub struct SignalRegistry {
    event: SignalEventShared,
    order: Option<Arc<Mutex<SignalOrderCurrent>>>,
    poll: Option<Arc<Mutex<PollQuery>>>,
    base: BaseRegistry<dyn UserSignal + Send>,
}

impl SignalRegistry {
    pub fn new() -> Self {
        Self {
            event: Arc::new(Mutex::new(SignalEvent::new())),
            order: None,
            poll: None,
            base: BaseRegistry::new(),
        }
    }

    pub fn end_load(&mut self, curr_langs: &[LanguageIdentifier]) -> Result<()> {
        let mut to_remove = Vec::new();

        for (sig_name, signal) in self.base.iter_mut() {
            if let Err(e) = signal.lock_it().end_load(curr_langs) {
                warn!(
                    "Signal \"{}\" had trouble in \"end_load\", will be disabled, error: {}",
                    &sig_name, e
                );

                to_remove.push(sig_name.to_owned());
            }
        }

        // Delete any signals which had problems during end_load
        for sig_name in &to_remove {
            self.base.remove_mangled(sig_name);
        }

        Ok(())
    }

    pub async fn call_loops(
        &mut self,
        config: &Config,
        curr_lang: &Vec<LanguageIdentifier>,
    ) -> Result<()> {
        let local = LocalSet::new();

        let spawn_on_local = |n: String, s: Arc<Mutex<dyn Signal + Send>>| {
            let event = self.event.clone();
            let config = config.clone();
            let curr_lang = curr_lang.clone();

            local.spawn_local(async move {
                let res = s.lock_it().event_loop(event, &config, &curr_lang).await;
                if let Err(e) = res {
                    error!("Signal '{}' had an error: {}", n, e.to_string());
                }
            });
        };

        let spawn_on_local_u = |n: String, s: Arc<Mutex<dyn UserSignal + Send>>| {
            let event = self.event.clone();
            let config = config.clone();
            let curr_lang = curr_lang.clone();

            local.spawn_local(async move {
                let res = s.lock_it().event_loop(event, &config, &curr_lang).await;
                if let Err(e) = res {
                    error!("Signal '{}' had an error: {}", n, e.to_string());
                }
            });
        };

        spawn_on_local(
            "order".into(),
            self.order
                .as_ref()
                .expect("Order signal had problems during init")
                .clone(),
        );
        spawn_on_local(
            "poll".into(),
            self.poll
                .as_ref()
                .expect("Poll signal had problems during init")
                .clone(),
        );
        for (sig_name, sig) in self.base.clone() {
            spawn_on_local_u(sig_name, sig);
        }

        local.await;
        Ok(())
    }

    pub fn set_order(&mut self, sig_order: Arc<Mutex<SignalOrderCurrent>>) -> Result<()> {
        self.order = Some(sig_order);
        Ok(())
    }

    pub fn set_poll(&mut self, sig_poll: Arc<Mutex<PollQuery>>) -> Result<()> {
        *POLL_SIGNAL.lock_it() = Some(sig_poll.clone());
        self.poll = Some(sig_poll);
        Ok(())
    }

    pub fn get_sig_order(&self) -> Option<&Arc<Mutex<SignalOrderCurrent>>> {
        self.order.as_ref()
    }

    delegate! {to self.base{
        pub fn insert(&mut self, skill_name: &str, sig_name: &str, signal: Arc<Mutex<dyn UserSignal + Send>>) -> Result<()>;
    }}
}
