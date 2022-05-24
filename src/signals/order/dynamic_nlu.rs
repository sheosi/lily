// Standard library
use std::collections::HashMap;
use std::fmt::{self, Debug};
use std::sync::{Mutex, Weak};

// This crate
use crate::actions::Action;
use crate::exts::LockIt;
use crate::nlu::{EntityDef, IntentData, NluManager, NluManagerStatic};
use crate::signals::{collections::NluMap, ActMap, ActionSet, SignalOrder};
use crate::vars::{mangle, NLU_TRAINING_DELAY};

// Other crates
use anyhow::{anyhow, Result};
use lazy_static::lazy_static;
use log::error;
use tokio::time::sleep_until;
use tokio::{
    spawn,
    sync::mpsc,
    time::{Duration, Instant},
};
use unic_langid::LanguageIdentifier;

lazy_static! {
    static ref DYNAMIC_NLU_CHANNEL: Mutex<Option<mpsc::Sender<DynamicNluRequest>>> =
        Mutex::new(None);
    static ref NEXT_NLU_COMPILATION: Mutex<Instant> = Mutex::new(Instant::now());
    static ref IS_NLU_COMPILATION_SCHEDULED: Mutex<bool> = Mutex::new(false);
}

#[derive(Debug)]
enum DynamicNluRequest {
    AddIntent {
        by_lang: HashMap<LanguageIdentifier, IntentData>,
        skill: String,
        intent_name: String,
    },

    AddEntity {
        skill: String,
        entity_name: String,
        by_lang: HashMap<LanguageIdentifier, EntityDef>,
    },

    EntityAddValue {
        skill: String,
        entity: String,
        value: String,
        langs: Vec<LanguageIdentifier>,
    },

    AddActionToIntent {
        skill: String,
        intent_name: String,
        action: WeakActionRef,
    },
}

struct WeakActionRef {
    pub act_ref: Weak<Mutex<dyn Action + Send>>,
}

impl Debug for WeakActionRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WeakActionRef").finish()
    }
}

fn init_dynamic_nlu() -> Result<mpsc::Receiver<DynamicNluRequest>> {
    let (producer, consumer) = mpsc::channel(100);

    (*DYNAMIC_NLU_CHANNEL.lock_it()) = Some(producer);

    Ok(consumer)
}

fn schedule_nlu_compilation<M: NluManager + NluManagerStatic + Debug + Send + 'static>(
    shared_nlu: Weak<Mutex<NluMap<M>>>,
    curr_langs: Vec<LanguageIdentifier>,
) {
    *NEXT_NLU_COMPILATION.lock_it() = Instant::now() + Duration::from_millis(NLU_TRAINING_DELAY);

    if *IS_NLU_COMPILATION_SCHEDULED.lock_it() {
        *IS_NLU_COMPILATION_SCHEDULED.lock_it() = true;

        spawn(async move {
            let next_compilation = *NEXT_NLU_COMPILATION.lock_it();
            while next_compilation > Instant::now() {
                sleep_until(next_compilation).await;
            }
            // Note: this on something like a multithreaded system might need a barrier
            // We uncheck this so soon since from now on bumping the time won't
            // be useful
            *IS_NLU_COMPILATION_SCHEDULED.lock_it() = false;

            let arc = shared_nlu.upgrade().unwrap();
            if let Err(e) = SignalOrder::end_loading(&arc, &curr_langs) {
                error!("Failed to end loading: {}", e);
            }
        });
    }
}

pub async fn on_dyn_nlu<M: NluManager + NluManagerStatic + Debug + Send + 'static>(
    shared_nlu: Weak<Mutex<NluMap<M>>>,
    intent_map: Weak<Mutex<ActMap>>,
    curr_langs: Vec<LanguageIdentifier>,
) -> Result<()> {
    let mut channel = init_dynamic_nlu()?;
    loop {
        match channel.recv().await.unwrap() {
            DynamicNluRequest::EntityAddValue {
                skill,
                entity,
                value,
                langs,
            } => {
                let langs = if langs.is_empty() {
                    curr_langs.clone()
                } else {
                    langs
                };

                let arc = shared_nlu.upgrade().unwrap();
                let mut m = arc.lock_it();
                for lang in langs {
                    let man = m.get_mut_nlu_man(&lang);
                    let mangled = mangle(&skill, &entity);
                    if let Err(e) = man.add_entity_value(&mangled, value.clone()) {
                        error!("Failed to add value to entity {}", e);
                    }
                }

                schedule_nlu_compilation(shared_nlu.clone(), curr_langs.clone());
            }

            DynamicNluRequest::AddIntent {
                by_lang,
                skill,
                intent_name,
            } => {
                let arc = shared_nlu.upgrade().unwrap();
                let mut m = arc.lock_it();
                for (lang, intent) in by_lang {
                    let man = m.get_mut_nlu_man(&lang);
                    let mangled = mangle(&skill, &intent_name);
                    man.add_intent(&mangled, intent.into_utterances(&skill));
                }

                schedule_nlu_compilation(shared_nlu.clone(), curr_langs.clone());
            }

            DynamicNluRequest::AddActionToIntent {
                action,
                intent_name,
                skill,
            } => intent_map.upgrade().unwrap().lock_it().add_mapping(
                &mangle(&skill, &intent_name),
                ActionSet::create(action.act_ref),
            ),

            DynamicNluRequest::AddEntity {
                skill,
                entity_name,
                by_lang,
            } => {
                let arc = shared_nlu.upgrade().unwrap();
                let mut m = arc.lock_it();

                for (lang, def) in by_lang {
                    let man = m.get_mut_nlu_man(&lang);
                    let mangled = mangle(&skill, &entity_name);
                    man.add_entity(mangled, def);
                }
            }
        }
    }
}

pub fn link_action_intent(
    intent_name: String,
    skill_name: String,
    action: Weak<Mutex<dyn Action + Send>>,
) -> Result<()> {
    send_in_channel(DynamicNluRequest::AddActionToIntent {
        skill: skill_name,
        intent_name,
        action: WeakActionRef { act_ref: action },
    })
}

pub fn add_entity_value(
    skill_name: String,
    entity_name: String,
    value: String,
    langs: Vec<LanguageIdentifier>,
) -> Result<()> {
    send_in_channel(DynamicNluRequest::EntityAddValue {
        skill: skill_name,
        entity: entity_name,
        value,
        langs,
    })
}

pub fn add_intent(
    by_lang: HashMap<LanguageIdentifier, IntentData>,
    skill: String,
    intent_name: String,
) -> Result<()> {
    send_in_channel(DynamicNluRequest::AddIntent {
        by_lang,
        skill,
        intent_name,
    })
}

pub fn add_entity(
    by_lang: HashMap<LanguageIdentifier, EntityDef>,
    skill: String,
    entity_name: String,
) -> Result<()> {
    send_in_channel(DynamicNluRequest::AddEntity {
        skill,
        entity_name,
        by_lang,
    })
}

fn send_in_channel(request: DynamicNluRequest) -> Result<()> {
    DYNAMIC_NLU_CHANNEL
        .lock_it()
        .as_ref()
        .unwrap()
        .try_send(request)
        .map_err(|e| anyhow!("Failed to send intent: {}", e))
}
