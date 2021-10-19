// Standard library
use std::collections::HashMap;
use std::fmt::{self, Debug};
use std::sync::{Mutex, Weak};

// This crate
use crate::actions::Action;
use crate::exts::LockIt;
use crate::nlu::{IntentData, NluManager, NluManagerStatic};
use crate::signals::{ActionSet, ActMap, collections::NluMap, SignalOrder};
use crate::vars::mangle;

// Other crates
use anyhow::Result;
use lazy_static::lazy_static;
use log::error;
use tokio::sync::mpsc;
use unic_langid::LanguageIdentifier;

lazy_static! {
    pub static ref DYNAMIC_NLU_CHANNEL: Mutex<Option<mpsc::Sender<DynamicNluRequest>>> =  Mutex::new(None);
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
                SignalOrder::end_loading(&arc, &curr_langs)?;
            }
            DynamicNluRequest::AddIntent(request) => {     
                let arc = shared_nlu.upgrade().unwrap();   
                let mut m = arc.lock_it();
                for (lang, intent) in request.by_lang {
                    let man = m.get_mut_nlu_man(&lang);
                    let mangled = mangle(&request.skill, &request.intent_name);
                    man.add_intent(&mangled,intent.into_utterances(&request.skill));
                }
                SignalOrder::end_loading(&arc, &curr_langs)?;
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