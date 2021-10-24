// Standard library
use std::collections::HashMap;
use std::fmt::{self, Debug};
use std::sync::{Arc, Mutex, Weak};

// This crate
use crate::actions::{ACT_REG, Action};
use crate::exts::LockIt;
use crate::nlu::{IntentData, NluManager, NluManagerStatic};
use crate::queries::{ActQuery, Query};
use crate::signals::{ActSignal, ActionSet, ActMap, collections::NluMap, SignalOrder, UserSignal};
use crate::vars::{mangle, NLU_TRAINING_DELAY};

// Other crates
use anyhow::Result;
use lazy_static::lazy_static;
use log::error;
use tokio::time::sleep_until;
use tokio::{spawn, sync::mpsc, time::{Duration, Instant}};
use unic_langid::LanguageIdentifier;

lazy_static! {
    pub static ref DYNAMIC_NLU_CHANNEL: Mutex<Option<mpsc::Sender<DynamicNluRequest>>> =  Mutex::new(None);
    pub static ref NEXT_NLU_COMPILATION: Mutex<Instant> = Mutex::new(Instant::now());
    pub static ref IS_NLU_COMPILATION_SCHEDULED: Mutex<bool> = Mutex::new(false);
}

#[derive(Debug)]
pub enum DynamicNluRequest {
    AddIntent(AddIntentRequest),
    EntityAddValue(EntityAddValueRequest),
    AddActionToIntent(AddActionToIntentRequest),
}

#[derive(Debug)]
pub struct AddIntentRequest {
    pub by_lang: HashMap<LanguageIdentifier, IntentData>,
    pub skill: String,
    pub intent_name: String,
}

#[derive(Debug)]
pub struct EntityAddValueRequest {
    pub skill: String,
    pub entity: String,
    pub value: String,
    pub langs: Vec<LanguageIdentifier>,
}


pub struct  AddActionToIntentRequest {
    pub skill: String,
    pub intent_name: String,
    pub action: Weak<Mutex<dyn Action + Send>>
}

impl Debug for AddActionToIntentRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AddActionToIntentRequest")
         .field("skill", &self.skill)
         .field("intent_name", &self.skill)
         .finish()
    }
}

pub fn init_dynamic_nlu() -> Result<mpsc::Receiver<DynamicNluRequest>> {
    let (producer, consumer) = mpsc::channel(100);

    (*DYNAMIC_NLU_CHANNEL.lock_it()) = Some(producer);

    Ok(consumer)
}

fn schedule_nlu_compilation<M: NluManager + NluManagerStatic + Debug + Send + 'static>(
    shared_nlu: Weak<Mutex<NluMap<M>>>,
    curr_langs: Vec<LanguageIdentifier>
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
    mut channel: mpsc::Receiver<DynamicNluRequest>,
    shared_nlu: Weak<Mutex<NluMap<M>>>,
    intent_map: Weak<Mutex<ActMap>>,
    curr_langs: Vec<LanguageIdentifier>,
) -> Result<()> {
    loop {
        match channel.recv().await.unwrap() {
            DynamicNluRequest::EntityAddValue(request) => {
                let langs = if request.langs.is_empty(){
                    curr_langs.clone()
                }
                else {
                    request.langs
                };

                let arc = shared_nlu.upgrade().unwrap();
                let mut m = arc.lock_it();
                for lang in langs {
                    let man = m.get_mut_nlu_man(&lang);
                    let mangled = mangle(&request.skill, &request.entity);
                    if let Err(e) = man.add_entity_value(&mangled, request.value.clone()) {
                        error!("Failed to add value to entity {}", e);
                    }
                }

                schedule_nlu_compilation(shared_nlu.clone(), curr_langs.clone());
            }
            DynamicNluRequest::AddIntent(request) => {     
                let arc = shared_nlu.upgrade().unwrap();   
                let mut m = arc.lock_it();
                for (lang, intent) in request.by_lang {
                    let man = m.get_mut_nlu_man(&lang);
                    let mangled = mangle(&request.skill, &request.intent_name);
                    man.add_intent(&mangled,intent.into_utterances(&request.skill));
                }

                schedule_nlu_compilation(shared_nlu.clone(), curr_langs.clone());
            }
            DynamicNluRequest::AddActionToIntent(request) => {
                intent_map.upgrade().unwrap().lock_it().add_mapping(
                    &mangle(&request.skill, &request.intent_name),
                    ActionSet::create(request.action)
                )
            }
        }
    }
}

pub fn link_action_intent(intent_name: String, skill_name: String,
    action: Weak<Mutex<dyn Action + Send>>) -> Result<()> {
    
    DYNAMIC_NLU_CHANNEL.lock_it().as_ref().unwrap().try_send(DynamicNluRequest::AddActionToIntent(AddActionToIntentRequest{
        skill: skill_name,
        intent_name,
        action
    }))?;

    Ok(())
}

pub fn link_signal_intent(intent_name: String, skill_name: String, signal_name: String,
    signal: Arc<Mutex<dyn UserSignal + Send>>) -> Result<()> {
    let arc = ActSignal::new(signal, signal_name);
    let weak = Arc::downgrade(&arc);
    ACT_REG.lock_it().insert(
        &skill_name,
        &format!("{}_signal_wrapper",intent_name),
        arc
    )?;
    
    DYNAMIC_NLU_CHANNEL.lock_it().as_ref().unwrap().try_send(DynamicNluRequest::AddActionToIntent(AddActionToIntentRequest{
        skill: skill_name,
        intent_name,
        action: weak
    }))?;

    Ok(())
}

pub fn link_query_intent(intent_name: String, skill_name: String,
    query_name: String, query: Arc<Mutex<dyn Query + Send>>) -> Result<()> {
    
    let arc = ActQuery::new(query, query_name);
    let weak = Arc::downgrade(&arc);
    ACT_REG.lock_it().insert(
        &skill_name,
        &format!("{}_query_wrapper",intent_name),
        arc
    )?;

    DYNAMIC_NLU_CHANNEL.lock_it().as_ref().unwrap().try_send(DynamicNluRequest::AddActionToIntent(AddActionToIntentRequest{
        skill: skill_name,
        intent_name,
        action:     weak
    }))?;

    Ok(())
}
