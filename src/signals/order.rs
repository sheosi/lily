// Standard library
use std::cell::RefCell;
use std::collections::HashMap;
use std::convert::TryInto;
use std::mem;
use std::rc::Rc;

// This crate
use crate::actions::ActionSet;
use crate::config::Config;
use crate::interfaces::{CURR_INTERFACE, DirectVoiceInterface, UserInterface};
use crate::nlu::{Nlu, NluManager, NluResponseSlot, NluUtterance, EntityInstance, EntityDef, EntityData};
use crate::python::{try_translate, try_translate_all};
use crate::stt::DecodeRes;
use crate::signals::{OrderMap, SignalEvent};
use crate::vars::*;

#[cfg(not(feature="devel_rasa_nlu"))]
use crate::nlu::{SnipsNlu, SnipsNluManager};
#[cfg(feature="devel_rasa_nlu")]
use crate::nlu::{RasaNlu, RasaNluManager};

// Other crates
use anyhow::{Result, anyhow};
use cpython::PyDict;
use log::{info, warn};
use serde::Deserialize;
use unic_langid::LanguageIdentifier;

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
#[cfg(not(feature = "devel_rasa_nlu"))]
pub struct SignalOrder {
    order_map: OrderMap,
    nlu_man: Option<SnipsNluManager>,
    nlu: Option<SnipsNlu>
}

#[cfg(feature = "devel_rasa_nlu")]
pub struct SignalOrder {
    order_map: OrderMap,
    nlu_man: Option<RasaNluManager>,
    nlu: Option<RasaNlu>
}

impl SignalOrder {
    #[cfg(not(feature = "devel_rasa_nlu"))]
    pub fn new() -> Self {
        SignalOrder {
            order_map: OrderMap::new(),
            nlu_man: Some(SnipsNluManager::new()),
            nlu: None
        }
    }

    #[cfg(feature = "devel_rasa_nlu")]
    pub fn new() -> Self {
        SignalOrder {
            order_map: OrderMap::new(),
            nlu_man: Some(RasaNluManager::new()),
            nlu: None
        }
    }
    
    pub fn add(&mut self, sig_arg: serde_yaml::Value, skill_name: &str, pkg_name: &str, act_set: Rc<RefCell<ActionSet>>) -> Result<()> {
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

    #[cfg(not(feature = "devel_rasa_nlu"))]
    pub fn end_loading(&mut self, curr_lang: &LanguageIdentifier) -> Result<()> {
        let res = match mem::replace(&mut self.nlu_man, None) {
            Some(nlu_man) => {
                nlu_man.train(&NLU_TRAIN_SET_PATH.resolve(), &NLU_ENGINE_PATH.resolve(), curr_lang)
            }
            None => {
                panic!("Called end_loading twice");
            }
        };

        info!("Init Nlu");
        self.nlu = Some(SnipsNlu::new(&NLU_ENGINE_PATH.resolve())?);
        
        res
    }
    

    #[cfg(feature = "devel_rasa_nlu")]
    pub fn end_loading(&mut self, curr_lang: &LanguageIdentifier) -> Result<()> {
        let model_path = NLU_RASA_PATH.resolve().join("data");
        let train_path = NLU_RASA_PATH.resolve().join("models").join("main_model.tar.gz");
        let res = match mem::replace(&mut self.nlu_man, None) {
            Some(nlu_man) => {
                nlu_man.train(&train_path, &model_path.join("main_model.json"), curr_lang)
            }
            None => {
                panic!("Called end_loading twice");
            }
        };

        info!("Init Nlu");
        self.nlu = Some(RasaNlu::new(&train_path)?);
        
        res
    }


    fn received_order(&mut self, decode_res: Option<DecodeRes>, event_signal: &mut SignalEvent, base_context: &PyDict) -> Result<()> {
        match decode_res {
            None => event_signal.call("empty_reco", &base_context)?,
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
                                    self.order_map.call_order(&intent_name, &slots_context)?;
                                    info!("Action called");
                                }
                                else {
                                    event_signal.call("unrecognized", &base_context)?;
                                }
                            }
                            else {
                                event_signal.call("unrecognized", &base_context)?;
                            }
                            
                        },
                        None => {
                            panic!("received_order can't be called before end_loading")
                        }
                    }
                }
                else {
                    event_signal.call("empty_reco", &base_context)?;
                }
            }
        }
    Ok(())
    }

    pub fn record_loop(&mut self, signal_event: &mut SignalEvent, config: &Config, base_context: &PyDict, curr_lang: &LanguageIdentifier) -> Result<()> {
        let mut interface = DirectVoiceInterface::new(curr_lang, config)?;
        CURR_INTERFACE.with(|itf|itf.replace(interface.get_output()));
        interface.interface_loop(config, signal_event, base_context, |d, s|{self.received_order(d, s, base_context)})
    }
}


fn add_slots(base_context: &PyDict, slots: Vec<NluResponseSlot>) -> Result<PyDict> {
    let gil = cpython::Python::acquire_gil();
    let py = gil.python();

    // What to do here if this fails?
    let result = base_context.copy(py).map_err(|py_err|anyhow!("Python error while copying base context: {:?}", py_err))?;

    for slot in slots.into_iter() {
        result.set_item(py, slot.name, slot.value).map_err(
            |py_err|anyhow!("Couldn't set name in base context: {:?}", py_err)
        )?;
    }

    Ok(result)

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
