// Standard library
 use std::collections::HashMap;
use std::convert::TryInto;
use std::mem;
use std::sync::{Arc, Mutex};

// This crate
use crate::actions::ActionSet;
use crate::config::Config;
use crate::interfaces::MqttInterface;
use crate::nlu::{EntityInstance, EntityDef, EntityData, Nlu, NluManager, NluManagerConf, NluManagerStatic, NluResponseSlot, NluUtterance};
use crate::python::{try_translate, try_translate_all};
use crate::stt::DecodeRes;
use crate::signals::{OrderMap, Signal, SignalEventShared};
use crate::vars::MIN_SCORE_FOR_ACTION;

// Other crates
use anyhow::{Result, anyhow};
use pyo3::{Py, conversion::IntoPy, types::PyDict, Python};
use log::{info, warn};
use serde::Deserialize;
use unic_langid::LanguageIdentifier;

#[cfg(not(feature = "devel_rasa_nlu"))]
use crate::nlu::SnipsNluManager;

#[cfg(feature = "devel_rasa_nlu")]
use crate::nlu::RasaNluManager;

#[derive(Deserialize)]
struct YamlEntityDef {
    data: Vec<String>
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OrderKind {
    Ref(String),
    Def(YamlEntityDef)
}

#[derive(Deserialize)]
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

pub fn add_order<N: NluManager>(
    sig_arg: serde_yaml::Value,
    nlu_man: &mut N,
    skill_name: &str,
    pkg_name: &str,
) -> Result<()> {
    let ord_data: OrderData = serde_yaml::from_value(sig_arg)?;
    match ord_data {
        OrderData::Direct(order_str) => {
            // into_iter lets us do a move operation
            let utts = try_translate_all(&order_str)?
                .into_iter()
                .map(|utt| NluUtterance::Direct(utt))
                .collect();
            nlu_man.add_intent(skill_name, utts);
        }
        OrderData::WithEntities { text, entities } => {
            let mut entities_res = HashMap::new();
            for (ent_name, ent_data) in entities.into_iter() {
                let ent_kind_name = match ent_data.kind {
                    OrderKind::Ref(name) => name,
                    OrderKind::Def(def) => {
                        let name = format!("_{}__{}_", pkg_name, ent_name);
                        nlu_man.add_entity(&name, def.try_into()?);
                        name
                    }
                };
                entities_res.insert(
                    ent_name,
                    EntityInstance {
                        kind: ent_kind_name,
                        example: {
                            match try_translate(&ent_data.example) {
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
            let utts = try_translate_all(&text)?
                .into_iter()
                .map(|utt| NluUtterance::WithEntities {
                    text: utt,
                    entities: entities_res.clone(),
                })
                .collect();
            nlu_man.add_intent(skill_name, utts);
        }
    }
    Ok(())
}

// Answers to a user order (either by voice or by text)

pub struct SignalOrder<M: NluManager + NluManagerStatic + NluManagerConf> {
    order_map: OrderMap,
    nlu_man: Option<M>,
    nlu: Option<M::NluType>
}

impl<M:NluManager + NluManagerStatic + NluManagerConf> SignalOrder<M> {
    pub fn new() -> Self {
        SignalOrder {
            order_map: OrderMap::new(),
            nlu_man: Some(M::new()),
            nlu: None
        }
    }

    pub fn end_loading(&mut self, curr_lang: &LanguageIdentifier) -> Result<()> {
        let (train_path, model_path) = M::get_paths();
        self.nlu = match mem::replace(&mut self.nlu_man, None) {
            Some(mut nlu_man) => {
                if M::is_lang_compatible(curr_lang) {
                    nlu_man.ready_lang(curr_lang)?;
                    Some(nlu_man.train(&train_path, &model_path.join("main_model.json"), curr_lang)?)
                }
                else {
                    Err(anyhow!("{} NLU is not compatible with the selected language", M::name()))?
                }
            }
            None => {
                panic!("Called end_loading twice");
            }
        };

        info!("Initted Nlu");
        
        Ok(())
    }

    fn received_order(&mut self, decode_res: Option<DecodeRes>, event_signal: SignalEventShared, base_context: &Py<PyDict>) -> Result<()> {
        match decode_res {
            None => event_signal.borrow_mut().call("empty_reco", base_context),
            Some(decode_res) => {
                
                if !decode_res.hypothesis.is_empty() {
                    match self.nlu {
                        Some(ref mut nlu) => {
                            let result = nlu.parse(&decode_res.hypothesis).map_err(|err|anyhow!("Failed to parse: {:?}", err))?;
                            info!("{:?}", result);

                            // Do action if at least we are 80% confident on
                            // what we got
                            if result.confidence >= MIN_SCORE_FOR_ACTION {
                                if let Some(intent_name) = result.name {
                                    info!("Let's call an action");
                                    let slots_context = add_slots(base_context,result.slots)?;
                                    self.order_map.call_order(&intent_name, &slots_context);
                                    info!("Action called");
                                }
                                else {
                                    event_signal.borrow_mut().call("unrecognized", &base_context);
                                }
                            }
                            else {
                                event_signal.borrow_mut().call("unrecognized", &base_context);
                            }
                            
                        },
                        None => {
                            panic!("received_order can't be called before end_loading")
                        }
                    }
                }
                else {
                    event_signal.borrow_mut().call("empty_reco", &base_context);
                }
            }
        }
    Ok(())
    }
 
    pub fn record_loop(&mut self, signal_event: SignalEventShared, config: &Config, base_context: &Py<PyDict>, curr_lang: &LanguageIdentifier) -> Result<()> {
        let mut interface = MqttInterface::new(curr_lang, config);
        interface.interface_loop(config, signal_event, base_context, |d, s|{self.received_order(d, s, base_context)})
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

impl<M:NluManager + NluManagerStatic + NluManagerConf> Signal for SignalOrder<M> {
    fn add(&mut self, sig_arg: serde_yaml::Value, skill_name: &str, pkg_name: &str, act_set: Arc<Mutex<ActionSet>>) -> Result<()> {
        match self.nlu_man {
            Some(ref mut nlu_man) => {
                add_order(sig_arg, nlu_man, skill_name, pkg_name)?;
                self.order_map.add_order(skill_name, act_set);

                Ok(())
            }
            None => {
                panic!("Called add_order after end_loading");
            }
        }
    }
    fn end_load(&mut self, curr_lang: &LanguageIdentifier) -> Result<()> {
        self.end_loading(curr_lang)
    }
    fn event_loop(&mut self, signal_event: SignalEventShared, config: &Config, base_context: &Py<PyDict>, curr_lang: &LanguageIdentifier) -> Result<()> {
        self.record_loop(signal_event, config, base_context, curr_lang)
    }
}

impl TryInto<EntityDef> for YamlEntityDef {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<EntityDef> {
        let mut data = Vec::new();

        for trans_data in self.data.into_iter(){
            let mut translations = try_translate_all(&trans_data)?;
            let value = translations.swap_remove(0);
            data.push(EntityData{value, synonyms: translations});
        }

        Ok(EntityDef{data, use_synonyms: true, automatically_extensible: true})
    }
}

#[cfg(not(feature = "devel_rasa_nlu"))]
pub fn new_signal_order() -> SignalOrder<SnipsNluManager> {
    SignalOrder::new()
}

#[cfg(feature = "devel_rasa_nlu")]
pub fn new_signal_order() -> SignalOrder<RasaNluManager> {
    SignalOrder::new()
}

