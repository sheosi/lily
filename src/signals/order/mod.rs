pub mod collections;
pub mod dev_mgmt;
pub mod dynamic_nlu;
pub mod mqtt;

mod server_actions;

// Standard library
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Arc, Mutex};

// This crate
use self::{
    dev_mgmt::SessionManager,
    dynamic_nlu::on_dyn_nlu,
    mqtt::MSG_OUTPUT,
    server_actions::{on_event, on_nlu_request},
};
use crate::actions::{
    Action, ActionAnswer, ActionContext, ActionSet, ContextData, MainAnswer, SatelliteData, ACT_REG,
};
use crate::config::Config;
use crate::exts::LockIt;
use crate::mqtt::MqttApi;
use crate::nlu::{EntityDef, IntentData, Nlu, NluManager, NluManagerStatic, NluResponseSlot};
use crate::queries::{ActQuery, Query};
use crate::signals::{
    collections::NluMap, ActMap, ActSignal, Signal, SignalEventShared, UserSignal,
};
use crate::stt::DecodeRes;
use crate::vars::{mangle, MIN_SCORE_FOR_ACTION};

// Other crates
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use log::{debug, error, info};
use tokio::{select, sync::mpsc};
use unic_langid::LanguageIdentifier;

#[cfg(not(feature = "devel_rasa_nlu"))]
use crate::nlu::SnipsNluManager;

#[cfg(feature = "devel_rasa_nlu")]
use crate::nlu::RasaNluManager;

#[derive(Debug)]
pub struct NluState<M: NluManager + NluManagerStatic + Send> {
    manager: M,
    nlu: Option<M::NluType>,
}

impl<M: NluManager + NluManagerStatic + Send> NluState<M> {
    fn get_mut_nlu_man(&mut self) -> &mut M {
        &mut self.manager
    }

    fn new(manager: M) -> Self {
        Self { manager, nlu: None }
    }
}
#[derive(Debug)]
pub struct SignalOrder<M: NluManager + NluManagerStatic + Debug + Send> {
    intent_map: Arc<Mutex<ActMap>>,
    nlu: Arc<Mutex<NluMap<M>>>,
    demangled_names: HashMap<String, String>,
}

impl<M: NluManager + NluManagerStatic + Debug + Send + 'static> SignalOrder<M> {
    pub fn new(langs: Vec<LanguageIdentifier>) -> Self {
        SignalOrder {
            intent_map: Arc::new(Mutex::new(ActMap::new())),
            nlu: Arc::new(Mutex::new(NluMap::new(langs))),
            demangled_names: HashMap::new(),
        }
    }

    pub async fn received_order(
        &mut self,
        decode_res: Option<DecodeRes>,
        event_signal: SignalEventShared,
        lang: &LanguageIdentifier,
        satellite: String,
    ) -> Result<bool> {
        debug!("Heard from user: {:?}", decode_res);

        fn make_context(lang: &LanguageIdentifier, uuid: String) -> ActionContext {
            ActionContext {
                locale: lang.to_string(),
                satellite: Some(SatelliteData { uuid }),
                data: ContextData::Event {
                    event: "__TO_FILL_THIS__".to_string(),
                },
            }
        }

        let ans = match decode_res {
            None => {
                event_signal
                    .lock_it()
                    .call("empty_reco", make_context(lang, satellite.clone()))
                    .await
            }
            Some(decode_res) => {
                if !decode_res.hypothesis.is_empty() {
                    let mut m = self.nlu.lock_it();
                    let nlu = m.get_nlu(lang);
                    let result = nlu
                        .parse(&decode_res.hypothesis)
                        .await
                        .map_err(|err| anyhow!("Failed to parse: {:?}", err))?;
                    info!("{:?}", result);

                    // Do action if at least we are 80% confident on
                    // what we got
                    if result.confidence >= MIN_SCORE_FOR_ACTION {
                        if let Some(intent_name) = result.name {
                            info!("Let's call an action");

                            let intent_data = crate::actions::IntentData {
                                name: self.demangle(&intent_name).to_string(),
                                input: decode_res.hypothesis,
                                slots: add_slots(result.slots),
                                confidence: result.confidence,
                            };

                            let mut intent_context = make_context(lang, satellite.clone());
                            intent_context.data = ContextData::Intent {
                                intent: intent_data,
                            };

                            let answers = self
                                .intent_map
                                .lock_it()
                                .call_mapping(&intent_name, &intent_context)
                                .await;
                            info!("Action called");
                            answers
                        } else {
                            event_signal
                                .lock_it()
                                .call("unrecognized", make_context(lang, satellite.clone()))
                                .await
                        }
                    } else {
                        event_signal
                            .lock_it()
                            .call("unrecognized", make_context(lang, satellite.clone()))
                            .await
                    }
                } else {
                    event_signal
                        .lock_it()
                        .call("empty_reco", make_context(lang, satellite.clone()))
                        .await
                }
            }
        };

        process_answers(ans, lang, satellite.clone())
    }

    pub fn end_loading(nlu: &Arc<Mutex<NluMap<M>>>, langs: &[LanguageIdentifier]) -> Result<()> {
        for lang in langs {
            let (train_path, model_path) = M::get_paths();
            let mut m = nlu.lock_it();
            let nlu = m.get_mut(lang)?;
            if M::is_lang_compatible(lang) {
                nlu.manager.ready_lang(lang)?;
                nlu.nlu = Some(nlu.manager.train(
                    &train_path,
                    &model_path.join("main_model.json"),
                    lang,
                )?);
            } else {
                return Err(anyhow!(
                    "{} NLU is not compatible with the selected language",
                    M::name()
                ));
            }

            info!("Initted Nlu");
        }

        Ok(())
    }
}

impl<M: NluManager + NluManagerStatic + Debug + Send> SignalOrder<M> {
    fn demangle<'a>(&'a self, mangled: &str) -> &'a str {
        self.demangled_names
            .get(mangled)
            .expect("Mangled name was not found")
    }

    fn add_intent(
        &mut self,
        sig_arg: Vec<(&LanguageIdentifier, IntentData)>,
        skill_name: &str,
        intent_name: &str,
        act_set: ActionSet,
    ) -> Result<()> {
        let mangled = mangle(skill_name, intent_name);

        {
            let mut nlu_grd = self.nlu.lock_it();
            for (lang, sig_arg) in sig_arg {
                nlu_grd.add_intent_to_nlu(sig_arg, &mangled, skill_name, lang)?;
            }
        }

        self.intent_map.lock_it().add_mapping(&mangled, act_set);
        self.demangled_names
            .insert(mangled, intent_name.to_string());
        Ok(())
    }

    pub fn add_intent_signal(
        &mut self,
        sig_arg: Vec<(&LanguageIdentifier, IntentData)>,
        skill_name: &str,
        intent_name: &str,
        signal_name: String,
        signal: Arc<Mutex<dyn UserSignal + Send>>,
    ) -> Result<()> {
        let arc = ActSignal::new(signal, signal_name);
        let weak = Arc::downgrade(&arc);
        ACT_REG
            .lock_it()
            .insert(skill_name, &format!("{}_signal_wrapper", intent_name), arc)?;

        self.add_intent(sig_arg, skill_name, intent_name, ActionSet::create(weak))?;
        Ok(())
    }

    pub fn add_intent_action(
        &mut self,
        sig_arg: Vec<(&LanguageIdentifier, IntentData)>,
        skill_name: &str,
        intent_name: &str,
        action: &Arc<Mutex<dyn Action + Send>>,
    ) -> Result<()> {
        let weak = Arc::downgrade(action);

        self.add_intent(sig_arg, &skill_name, &intent_name, ActionSet::create(weak))?;
        Ok(())
    }

    pub fn add_intent_query(
        &mut self,
        sig_arg: Vec<(&LanguageIdentifier, IntentData)>,
        skill_name: &str,
        intent_name: &str,
        query_name: String,
        query: Arc<Mutex<dyn Query + Send>>,
    ) -> Result<()> {
        let arc = ActQuery::new(query, query_name);
        let weak = Arc::downgrade(&arc);
        ACT_REG
            .lock_it()
            .insert(&skill_name, &format!("{}_query_wrapper", intent_name), arc)?;

        self.add_intent(sig_arg, &skill_name, &intent_name, ActionSet::create(weak))?;

        Ok(())
    }

    pub fn add_slot_type(&mut self, type_name: String, data: EntityDef, lang: &LanguageIdentifier) {
        let mut m = self.nlu.lock_it();
        m.get_mut_nlu_man(lang).add_entity(type_name, data);
    }
}

#[async_trait(?Send)]
impl<M: NluManager + NluManagerStatic + Debug + Send + 'static> Signal for SignalOrder<M> {
    fn end_load(&mut self, curr_langs: &[LanguageIdentifier]) -> Result<()> {
        Self::end_loading(&self.nlu, curr_langs)
    }

    async fn event_loop(
        &mut self,
        signal_event: SignalEventShared,
        config: &Config,
        curr_langs: &[LanguageIdentifier],
    ) -> Result<()> {
        let def_lang = curr_langs.get(0);
        let mut mqtt = MqttApi::new(def_lang.expect("We need at least one language").clone())?;

        let (nlu_sender, nlu_receiver) = mpsc::channel(100);
        let (event_sender, event_receiver) = mpsc::channel(100);
        let sessions = Arc::new(Mutex::new(SessionManager::new()));
        let dyn_ent_fut = on_dyn_nlu(
            Arc::downgrade(&self.nlu),
            Arc::downgrade(&self.intent_map),
            curr_langs.to_vec(),
        );
        select! {
            e = dyn_ent_fut => {Err(anyhow!("Dynamic entitying failed: {:?}",e))}
            e = mqtt.api_loop(config, curr_langs, def_lang, sessions.clone(), nlu_sender, event_sender) => {e}
            e = on_nlu_request(config, nlu_receiver, signal_event.clone(), curr_langs, self, sessions) => {Err(anyhow!("Nlu request failed: {:?}", e))}
            e = on_event(event_receiver, signal_event, def_lang) => {Err(anyhow!("Event handling failed: {:?}", e))}
        }
    }
}

/// Transform the in the response into a HashMap for sending
fn add_slots(slots: Vec<NluResponseSlot>) -> HashMap<String, String> {
    let mut result = HashMap::new();
    for slot in slots.into_iter() {
        result.insert(slot.name, slot.value);
    }

    result
}

// Response
fn process_answer(ans: ActionAnswer, lang: &LanguageIdentifier, uuid: String) -> Result<()> {
    MSG_OUTPUT.with::<_, Result<()>>(|m| {
        match *m.borrow_mut() {
            Some(ref mut output) => {
                match ans.answer {
                    MainAnswer::Sound(s) => output.send_audio(s, uuid),
                    MainAnswer::Text(t) => output.answer(t, lang, uuid),
                }?;
            }
            _ => {}
        };
        Ok(())
    })
}

// Response
pub fn process_answers(
    ans: Option<Vec<ActionAnswer>>,
    lang: &LanguageIdentifier,
    satellite: String,
) -> Result<bool> {
    if let Some(answers) = ans {
        let (ans, errs): (Vec<_>, Vec<Result<bool>>) = answers
            .into_iter()
            .map(|a| {
                let s = a.should_end_session;
                process_answer(a, lang, satellite.clone())?;
                Ok(s)
            })
            .partition(Result::is_err);

        errs.into_iter()
            .map(|e| e.err().unwrap())
            .for_each(|e| error!("There was an error while handling answer: {}", e));

        // By default, the session is ended, it is kept alive as soon as someone asks not to do that
        let s = ans
            .into_iter()
            .map(|s| s.ok().unwrap())
            .fold(true, |a, b| !(!a | !b));
        Ok(s)
    } else {
        // Should end session?
        Ok(true)
    }
}

#[cfg(not(feature = "devel_rasa_nlu"))]
pub type CurrentNluManager = SnipsNluManager;
#[cfg(feature = "devel_rasa_nlu")]
pub type CurrentNluManager = RasaNluManager;

pub type SignalOrderCurrent = SignalOrder<CurrentNluManager>;

pub fn new_signal_order(langs: Vec<LanguageIdentifier>) -> SignalOrder<CurrentNluManager> {
    SignalOrder::new(langs)
}
