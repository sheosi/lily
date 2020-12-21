pub mod server_interface;

// Standard library
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// This crate
use crate::actions::ActionSet;
use crate::config::Config;
use crate::nlu::{EntityInstance, EntityDef, EntityData, Nlu, NluManager, NluManagerConf, NluManagerStatic, NluResponseSlot, NluUtterance};
use crate::python::{try_translate, try_translate_all};
use crate::stt::DecodeRes;
use crate::signals::{MakeSendable, OrderMap, Signal, SignalEventShared};
use crate::vars::MIN_SCORE_FOR_ACTION;
use self::server_interface::MqttInterface;

// Other crates
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use pyo3::{Py, conversion::IntoPy, types::PyDict, Python};
use log::{info, warn};
use serde::Deserialize;
use unic_langid::LanguageIdentifier;

#[cfg(not(feature = "devel_rasa_nlu"))]
use crate::nlu::SnipsNluManager;

#[cfg(feature = "devel_rasa_nlu")]
use crate::nlu::RasaNluManager;

#[derive(Clone, Deserialize)]
struct YamlEntityDef {
    data: Vec<String>
}

#[derive(Clone, Deserialize)]
#[serde(untagged)]
enum OrderKind {
    Ref(String),
    Def(YamlEntityDef)
}

#[derive(Clone, Deserialize)]
struct OrderEntity {
    kind: OrderKind,
    example: String
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OrderData {
    Direct(String),
    WithEntities{text: String, entities: HashMap<String, OrderEntity>}
}

fn add_order<N: NluManager + NluManagerStatic + NluManagerConf>(
    sig_arg: serde_yaml::Value,
    nlu_man: &mut HashMap<LanguageIdentifier, NluState<N>>,
    skill_name: &str,
    pkg_name: &str,
    langs: &Vec<LanguageIdentifier>
) -> Result<()> {
    let ord_data: OrderData = serde_yaml::from_value(sig_arg)?;
    match ord_data {
        OrderData::Direct(order_str) => {
            for lang in langs {
                // into_iter lets us do a move operation
                let utts = try_translate_all(&order_str, &lang.to_string())?
                    .into_iter()
                    .map(|utt| NluUtterance::Direct(utt))
                    .collect();
                nlu_man.get_mut(lang).expect("Nlu already trained").get_mut_nlu_man()
                .add_intent(skill_name, utts);
            }
            
        }
        OrderData::WithEntities { text, entities } => {
            for lang in langs {
                let mut entities_res:HashMap<String, EntityInstance> = HashMap::new();
                for (ent_name, ent_data) in entities.iter() {
                    let ent_kind_name = match ent_data.kind.clone() {
                        OrderKind::Ref(name) => name,
                        OrderKind::Def(def) => {
                            let name = format!("_{}__{}_", pkg_name, ent_name);
                            nlu_man.get_mut(lang).expect("Nlu already trained").get_mut_nlu_man()
                            .add_entity(&name, def.try_into_with_trans(lang)?);
                            name
                        }
                    };
                    entities_res.insert(
                        ent_name.to_string(),
                        EntityInstance {
                            kind: ent_kind_name,
                            example: {
                                match try_translate(&ent_data.example, &lang.to_string()) {
                                    Ok(trans) =>  trans,
                                    Err(err) => {
                                        warn!("Failed to do translation of \"{}\", error: {:?}", &ent_data.example, err);
                                        ent_data.example.clone()
                                    }
                                }
                            },
                        },
                    );
                }
                let lang_str = lang.to_string();
                let utts = try_translate_all(&text, &lang_str)?
                    .into_iter()
                    .map(|utt| NluUtterance::WithEntities {
                        text: utt,
                        entities: entities_res.clone(),
                    })
                    .collect();
                nlu_man.get_mut(lang).expect(&format!("lang {} wasn't provided before", &lang_str)).get_mut_nlu_man()
                .add_intent(skill_name, utts);
            }
        }
    }
    Ok(())
}
enum NluState<M: NluManager + NluManagerStatic + NluManagerConf> {
    Training(M), Done(M::NluType), InProcess
}

impl<M: NluManager + NluManagerStatic + NluManagerConf> NluState<M> {
    fn get_mut_nlu_man(&mut self) -> &mut M {
        match self {
            NluState::Training(m) => {m},
            NluState::Done(_)=> {panic!("Can't get Nlu Manager when the NLU it's already trained")},
            NluState::InProcess => {panic!("Can't call this while training the NLU")}
        }
    }
}

pub struct SignalOrder<M: NluManager + NluManagerStatic + NluManagerConf> {
    intent_map: OrderMap,
    nlu: HashMap<LanguageIdentifier, NluState<M>>,
    langs: Vec<LanguageIdentifier>
}

impl<M:NluManager + NluManagerStatic + NluManagerConf> SignalOrder<M> {
    pub fn new(langs: Vec<LanguageIdentifier>) -> Self {
        let mut managers = HashMap::new();
        for lang in &langs {
            managers.insert(lang.to_owned(), NluState::Training(M::new()));
        }
        SignalOrder {
            intent_map: OrderMap::new(),
            nlu: managers,
            langs
        }
    }

    pub fn end_loading(&mut self) -> Result<()> {
        for lang in &self.langs {
            let (train_path, model_path) = M::get_paths();
            let val  = match self.nlu.insert(lang.clone(), NluState::InProcess).unwrap() {
                NluState::Training(mut nlu_man) => {
                    if M::is_lang_compatible(lang) {
                        nlu_man.ready_lang(lang)?;
                        NluState::Done(nlu_man.train(&train_path, &model_path.join("main_model.json"), lang)?)
                    }
                    else {
                        Err(anyhow!("{} NLU is not compatible with the selected language", M::name()))?
                    }
                }
                _ => {
                    panic!("Called end_loading twice");
                }
            };
            self.nlu.insert(lang.to_owned(), val);

            info!("Initted Nlu");
        }

        Ok(())
    }

    pub async fn received_order(&mut self, decode_res: Option<DecodeRes>, event_signal: SignalEventShared, base_context: &Py<PyDict>, lang: &LanguageIdentifier) -> Result<()> {
        match decode_res {
            None => event_signal.lock().sendable()?.call("empty_reco", base_context),
            Some(decode_res) => {

                if !decode_res.hypothesis.is_empty() {
                    match self.nlu.get_mut(&lang).unwrap() {
                        NluState::Done(ref mut nlu) => {
                            let result = nlu.parse(&decode_res.hypothesis).await.map_err(|err|anyhow!("Failed to parse: {:?}", err))?;
                            info!("{:?}", result);

                            // Do action if at least we are 80% confident on
                            // what we got
                            if result.confidence >= MIN_SCORE_FOR_ACTION {
                                if let Some(intent_name) = result.name {
                                    info!("Let's call an action");
                                    let slots_context = add_slots(base_context,result.slots)?;
                                    self.intent_map.call_order(&intent_name, &slots_context);
                                    info!("Action called");
                                }
                                else {
                                    event_signal.lock().sendable()?.call("unrecognized", &base_context);
                                }
                            }
                            else {
                                event_signal.lock().sendable()?.call("unrecognized", &base_context);
                            }

                        },
                        _ => {
                            panic!("received_order can't be called before end_loading")
                        }
                    }
                }
                else {
                    event_signal.lock().sendable()?.call("empty_reco", &base_context);
                }
            }
        }
    Ok(())
    }

    pub async fn record_loop(&mut self, signal_event: SignalEventShared, config: &Config, base_context: &Py<PyDict>, curr_lang: &Vec<LanguageIdentifier>) -> Result<()> {
        let mut interface = MqttInterface::new(curr_lang)?;
        interface.interface_loop(config, signal_event, base_context, self).await
    }
}


fn add_slots(base_context: &Py<PyDict>, slots: Vec<NluResponseSlot>) -> Result<Py<PyDict>> {
    let gil = Python::acquire_gil();
    let py = gil.python();

    // What to do here if this fails?
    let result = base_context.as_ref(py).copy()?;

    for slot in slots.into_iter() {
        result.set_item(slot.name, slot.value).map_err(
            |py_err|anyhow!("Couldn't set name in base context: {:?}", py_err)
        )?;
    }



    Ok(result.into_py(py))

}

#[async_trait(?Send)]
impl<M:NluManager + NluManagerStatic + NluManagerConf> Signal for SignalOrder<M> {
    fn add(&mut self, sig_arg: serde_yaml::Value, skill_name: &str, pkg_name: &str, act_set: Arc<Mutex<ActionSet>>) -> Result<()> {

        add_order(sig_arg, &mut self.nlu, skill_name, pkg_name, &self.langs)?;
        self.intent_map.add_order(skill_name, act_set);

        Ok(())

    }
    fn end_load(&mut self, _curr_lang: &Vec<LanguageIdentifier>) -> Result<()> {
        self.end_loading()
    }
    async fn event_loop(&mut self, signal_event: SignalEventShared, config: &Config, base_context: &Py<PyDict>, curr_lang: &Vec<LanguageIdentifier>) -> Result<()> {
        self.record_loop(signal_event, config, base_context, curr_lang).await
    }
}

impl YamlEntityDef {
    fn try_into_with_trans(self, lang: &LanguageIdentifier) -> Result<EntityDef> {
        let mut data = Vec::new();

        for trans_data in self.data.into_iter(){
            let mut translations = try_translate_all(&trans_data, &lang.to_string())?;
            let value = translations.swap_remove(0);
            data.push(EntityData{value, synonyms: translations});
        }

        Ok(EntityDef{data, use_synonyms: true, automatically_extensible: true})
    }
}

#[cfg(not(feature = "devel_rasa_nlu"))]
pub fn new_signal_order(langs: Vec<LanguageIdentifier>) -> SignalOrder<SnipsNluManager> {
    SignalOrder::new(langs)
}

#[cfg(feature = "devel_rasa_nlu")]
pub fn new_signal_order(langs: Vec<LanguageIdentifier>) -> SignalOrder<RasaNluManager> {
    SignalOrder::new(langs)
}

