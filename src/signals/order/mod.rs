pub mod collections;
pub mod dynamic_nlu;
pub mod server_interface;

// Standard library
use std::collections::HashMap;
use std::fmt::Debug;
use std::mem::replace;
use std::sync::{Arc, Mutex};

// This crate
use crate::actions::{ActionAnswer, ActionContext, ActionSet, MainAnswer};
use crate::config::Config;
use crate::exts::LockIt;
use crate::nlu::{EntityDef, EntityInstance, Nlu, NluManager, NluManagerStatic, NluResponseSlot, NluUtterance};
use crate::python::try_translate;
use crate::stt::DecodeRes;
use crate::signals::{collections::{IntentData, OrderKind}, dynamic_nlu::EntityAddValueRequest, ActMap, Signal, SignalEventShared};
use crate::vars::{mangle, MIN_SCORE_FOR_ACTION};
use self::server_interface::{MqttInterface, MSG_OUTPUT, on_event, on_nlu_request, SessionManager};

// Other crates
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use log::{debug, info, error, warn};
use tokio::{select, sync::mpsc};
use unic_langid::LanguageIdentifier;

#[cfg(not(feature = "devel_rasa_nlu"))]
use crate::nlu::SnipsNluManager;

#[cfg(feature = "devel_rasa_nlu")]
use crate::nlu::RasaNluManager;

fn add_intent_to_nlu<N: NluManager + NluManagerStatic + Send>(
    nlu_man: &mut HashMap<LanguageIdentifier, NluState<N>>,
    sig_arg: IntentData,
    intent_name: &str,
    skill_name: &str,
    langs: &Vec<LanguageIdentifier>
) -> Result<()> {
    
    for lang in langs {

        //First, register all slots
        let mut slots_res:HashMap<String, EntityInstance> = HashMap::new();
        for (slot_name, slot_data) in sig_arg.slots.iter() {

            // Handle that slot types might be defined on the spot
            let ent_kind_name = match slot_data.slot_type.clone() {
                OrderKind::Ref(name) => name,
                OrderKind::Def(def) => {
                    let name = format!("_{}__{}_", skill_name, slot_name);
                    nlu_man.get_mut(lang).expect("Language not registered").get_mut_nlu_man()
                    .add_entity(&name, def.try_into_with_trans(lang)?);
                    name
                }
            };

            let slot_example = "".to_string();

            slots_res.insert(
                slot_name.to_string(),
                EntityInstance {
                    kind: ent_kind_name,
                    example: {
                        try_translate(&slot_example, &lang.to_string()).unwrap_or_else(|e|{
                            warn!("Failed to do translation of \"{}\", error: {:?}", &slot_example, e);
                            slot_example
                        })
                    },
                },
            );
        }

        // Now register all utterances
        match sig_arg.utts.clone().into_translation(lang) {
            Ok(t) => {
                let utts = t.into_iter().map(|utt|
                if slots_res.is_empty() {
                    NluUtterance::Direct(utt)
                }
                else {
                    NluUtterance::WithEntities {
                        text: utt,
                        entities: slots_res.clone(),
                    }
                }).collect();

                nlu_man.get_mut(lang).expect("Input language was not present before").get_mut_nlu_man()
                .add_intent(intent_name, utts);
            }
            Err(failed) => {
                if failed.len() == 1 {
                    error!("Sample '{}' of '{}'  couldn't be translated", failed[0], skill_name)
                }
                else {
                    error!("Samples '{}' of '{}' couldn't be translated", failed.join(", "), skill_name)
                }
            }

        }
            
    }
    Ok(())
}


#[derive(Debug)]
pub struct NluState<M: NluManager + NluManagerStatic + Send> {
    manager: M,
    nlu: Option<M::NluType>
}

impl<M: NluManager + NluManagerStatic + Send> NluState<M> {
    fn get_mut_nlu_man(&mut self) -> &mut M {
        &mut self.manager
    }

    fn new(manager: M) -> Self {
        Self {manager, nlu: None}
    }
}

#[derive(Debug)]
pub struct SignalOrder<M: NluManager + NluManagerStatic + Debug + Send> {
    intent_map: ActMap,
    nlu: Arc<Mutex<HashMap<LanguageIdentifier, NluState<M>>>>,
    demangled_names: HashMap<String, String>,
    dyn_entities: Option<mpsc::Receiver<EntityAddValueRequest>>,
    langs: Vec<LanguageIdentifier>
}

impl<M:NluManager + NluManagerStatic + Debug + Send + 'static> SignalOrder<M> {
    pub fn new(langs: Vec<LanguageIdentifier>, consumer: mpsc::Receiver<EntityAddValueRequest>) -> Self {
        let mut managers = HashMap::new();

        // Create a nlu manager per language
        for lang in &langs {
            managers.insert(lang.to_owned(), NluState::new(M::new()));
        }
        SignalOrder {
            intent_map: ActMap::new(),
            nlu: Arc::new(Mutex::new(managers)),
            demangled_names: HashMap::new(),
            dyn_entities: Some(consumer),
            langs
        }
    }

    pub fn end_loading(nlu: Arc<Mutex<HashMap<LanguageIdentifier, NluState<M>>>>,langs: &Vec<LanguageIdentifier>) -> Result<()> {
        for lang in langs {
            let (train_path, model_path) = M::get_paths();
            let err = || {anyhow!("Received language '{}' has not been registered", lang.to_string())};
            let mut m = nlu.lock_it();
            let nlu  = m.get_mut(lang).ok_or_else(err)?;
            if M::is_lang_compatible(lang) {
                nlu.manager.ready_lang(lang)?;
                nlu.nlu = Some(nlu.manager.train(&train_path, &model_path.join("main_model.json"), lang)?);
            }
            else {
                Err(anyhow!("{} NLU is not compatible with the selected language", M::name()))?
            }

            info!("Initted Nlu");
        }

        Ok(())
    }

    pub async fn received_order(&mut self, decode_res: Option<DecodeRes>, event_signal: SignalEventShared, base_context: &ActionContext, lang: &LanguageIdentifier, satellite: String) -> Result<bool> {
        debug!("Heard from user: {:?}", decode_res);

        let ans = match decode_res {
            None => event_signal.lock_it().call("empty_reco", base_context.clone()),
            Some(decode_res) => {

                if !decode_res.hypothesis.is_empty() {
                    const ERR_MSG: &str = "Received language to the NLU was not registered";
                    const NO_NLU_MSG: &str = "received_order can't be called before end_loading";
                    let mut m = self.nlu.lock_it();
                    let nlu = &m.get_mut(&lang).expect(ERR_MSG).nlu.as_mut().expect(NO_NLU_MSG);
                    let result = nlu.parse(&decode_res.hypothesis).await.map_err(|err|anyhow!("Failed to parse: {:?}", err))?;
                    info!("{:?}", result);

                    // Do action if at least we are 80% confident on
                    // what we got
                    if result.confidence >= MIN_SCORE_FOR_ACTION {
                        if let Some(intent_name) = result.name {
                            info!("Let's call an action");

                            let slots_data =  add_slots(&ActionContext::new(),result.slots);

                            let mut intent_data = ActionContext::new();
                            intent_data.set_str("name".to_string(), self.demangle(&intent_name).to_string());
                            intent_data.set_dict("slots".to_string(), slots_data);

                            let mut intent_context = base_context.clone();
                            intent_context.set_str("type".to_string(), "intent".to_string());
                            intent_context.set_dict("intent".to_string(), intent_data);
                            
                            let answers = self.intent_map.call_mapping(&intent_name, &intent_context);
                            info!("Action called");
                            answers
                        }
                        else {
                            event_signal.lock_it().call("unrecognized", base_context.clone())
                        }
                    }
                    else {
                        event_signal.lock_it().call("unrecognized", base_context.clone())
                    }
                }
                else {
                    event_signal.lock_it().call("empty_reco", base_context.clone())
                }
            }
        };
        
        process_answers(ans, lang, satellite.clone())
    }
}

impl<M:NluManager + NluManagerStatic + Debug + Send>  SignalOrder<M> {
    fn demangle<'a>(&'a self, mangled: &str) -> &'a str {
        self.demangled_names.get(mangled).expect("Mangled name was not found")
    }
    pub fn add_intent(&mut self, sig_arg: IntentData, intent_name: &str, skill_name: &str, act_set: Arc<Mutex<ActionSet>>) -> Result<()> {
        let mangled = mangle(skill_name, intent_name);
        add_intent_to_nlu(&mut self.nlu.lock_it(), sig_arg, &mangled, skill_name, &self.langs)?;
        self.intent_map.add_mapping(&mangled, act_set);
        self.demangled_names.insert(mangled, intent_name.to_string());
        Ok(())
    }

    pub fn add_slot_type(&mut self, type_name: String, data: EntityDef) -> Result<()> {
        for lang in &self.langs {
            let mut m = self.nlu.lock_it();
            let nlu_man= m.get_mut(lang).expect("Language not registered").get_mut_nlu_man();
            let trans_data = data.clone().into_translation(lang)?;
            nlu_man.add_entity(&type_name, trans_data);
        }
        Ok(())
    }
}


#[async_trait(?Send)]
impl<M:NluManager + NluManagerStatic + Debug + Send + 'static> Signal for SignalOrder<M> {
    fn end_load(&mut self, _curr_lang: &Vec<LanguageIdentifier>) -> Result<()> {
        Self::end_loading(self.nlu.clone(), &self.langs)
    }

    async fn event_loop(&mut self, 
        signal_event: SignalEventShared,
        config: &Config,
        base_context: &ActionContext,
        curr_langs: &Vec<LanguageIdentifier>
    ) -> Result<()> {
        let mut interface = MqttInterface::new()?;

        // Dyn entities data
        
        async fn on_dyn_entity<M: NluManager + NluManagerStatic + Debug + Send + 'static>(
            mut channel: mpsc::Receiver<EntityAddValueRequest>,
            shared_nlu: Arc<Mutex<HashMap<LanguageIdentifier, NluState<M>>>>,
            curr_langs: Vec<LanguageIdentifier>,
        ) -> Result<()> {
            loop {
                let request= channel.recv().await.unwrap();
                let langs = if request.langs.is_empty(){
                    curr_langs.clone()
                }
                else {
                    request.langs
                };

                let mut m = shared_nlu.lock_it();
                for lang in langs {
                    let man = m.get_mut(&lang).expect("Language not registered").get_mut_nlu_man();
                    let mangled = mangle(&request.skill, &request.entity);
                    if let Err(e) = man.add_entity_value(&mangled, request.value.clone()) {
                        error!("Failed to add value to entity {}", e);
                    }

                    SignalOrder::end_loading(shared_nlu.clone(), &curr_langs)?;
                }
            }
        }

        let (nlu_sender, nlu_receiver) = tokio::sync::mpsc::channel(100);
        let (event_sender, event_receiver) = tokio::sync::mpsc::channel(100);
        let sessions = Arc::new(Mutex::new(SessionManager::new()));
        let def_lang = curr_langs.get(0);
        let dyn_ent_fut = on_dyn_entity(
             replace(&mut self.dyn_entities, None).expect("Dyn_entities already consumed"),
             self.nlu.clone(),
             curr_langs.clone()
        );
        select!{
            e = dyn_ent_fut => {Err(anyhow!("Dynamic entitying failed: {:?}",e))}
            e = interface.interface_loop(config, curr_langs, def_lang, sessions.clone(), nlu_sender, event_sender) => {e}
            e = on_nlu_request(config, nlu_receiver, signal_event.clone(), curr_langs, self, base_context, sessions) => {Err(anyhow!("Nlu request failed: {:?}", e))}
            e = on_event(event_receiver, signal_event, def_lang, base_context) => {Err(anyhow!("Event handling failed: {:?}", e))}
        }
    }
}

fn add_slots(base_context: &ActionContext, slots: Vec<NluResponseSlot>) -> ActionContext {
    let mut result = base_context.clone();
    for slot in slots.into_iter() {
        result.set_str(slot.name, slot.value);
    }

    result
}

fn process_answer(ans: ActionAnswer, lang: &LanguageIdentifier, uuid: String) -> Result<()> {
    MSG_OUTPUT. with::<_,Result<()>>(|m|{match *m.borrow_mut() {
        Some(ref mut output) => {
            match ans.answer {
                MainAnswer::Sound(s) => {
                    output.send_audio(s, uuid)
                }
                MainAnswer::Text(t) => {
                    output.answer(t, lang, uuid)
                }
            }?;  
        }
        _=>{}
    };
    Ok(())
    })
}

pub fn process_answers(
    ans: Option<Vec<ActionAnswer>>,
    lang: &LanguageIdentifier,
    satellite: String) -> Result<bool> {
    if let Some(answers) = ans {
        let (ans,errs): (Vec<_>, Vec<Result<bool>>) = answers.into_iter()
        .map(|a|{
            let s = a.should_end_session;
            process_answer(a, lang, satellite.clone())?;
            Ok(s)
        })
        .partition(Result::is_err);

        errs.into_iter()
        .map(|e|e.err().unwrap())
        .for_each(|e| error!("There was an error while handling answer: {}", e));

        // By default, the session is ended, it is kept alive as soon as someone asks not to do that
        let s = ans.into_iter().map(|s|s.ok().unwrap()).fold(true,|a,b|{!(!a|!b)});
        Ok(s)
    }
    else {
        // Should end session?
        Ok(true)
    }

}



#[cfg(not(feature = "devel_rasa_nlu"))]
pub type CurrentNluManager = SnipsNluManager;
#[cfg(feature = "devel_rasa_nlu")]
pub type CurrentNluManager = RasaNluManager;

pub type SignalOrderCurrent = SignalOrder<CurrentNluManager>;

pub fn new_signal_order(langs: Vec<LanguageIdentifier>, consumer: mpsc::Receiver<EntityAddValueRequest>) -> SignalOrder<CurrentNluManager> {
    SignalOrder::new(langs, consumer)
}

