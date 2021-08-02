// Standard library
use std::collections::HashMap;
use std::fmt::Debug;

// This crate
use crate::exts::StringList;
use crate::signals::order::NluState;
use crate::nlu::{EntityData, EntityDef, EntityInstance, NluManager, NluManagerStatic, NluUtterance};
use crate::python::{try_translate, try_translate_all};

// Other crates
use anyhow::{anyhow, Result};
use serde::Deserialize;
use log::{error, warn};
use unic_langid::LanguageIdentifier;


fn false_val() -> bool {false}
fn none() -> Option<String> {None}

/*** Config ********************************************************************/
#[derive(Clone, Debug, Deserialize)]
pub enum Hook {
    #[serde(rename="query")]
    Query(String),
    #[serde(rename="action")]
    Action(String),
    #[serde(rename="signal")]
    Signal(String)
}

fn empty_map() -> HashMap<String, SlotData> {
    HashMap::new()
}

#[derive(Debug, Deserialize)]
pub struct IntentData {

    #[serde(alias = "samples", alias = "sample")]
    pub utts:  StringList,
    #[serde(default="empty_map")]
    pub slots: HashMap<String, SlotData>,
    #[serde(flatten)]
    pub hook: Hook
}

#[derive(Debug, Deserialize)]
pub struct SlotData {
    #[serde(rename="type")]
    pub slot_type: OrderKind,
    #[serde(default="false_val")]
    pub required: bool,
    #[serde(default="none")]
    pub prompt: Option<String>,
    #[serde(default="none")]
    pub reprompt: Option<String>
}

#[derive(Clone, Deserialize)]
struct OrderEntity {
    kind: OrderKind,
    example: String
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum OrderKind {
    Ref(String),
    Def(YamlEntityDef)
}

#[derive(Debug, Clone, Deserialize)]
pub struct YamlEntityDef {
    data: Vec<String>
}

impl YamlEntityDef {
    pub fn try_into_with_trans(self, lang: &LanguageIdentifier) -> Result<EntityDef> {
        let mut data = Vec::new();

        for trans_data in self.data.into_iter(){
            let mut translations = try_translate_all(&trans_data, &lang.to_string())?;
            let value = translations.swap_remove(0);
            data.push(EntityData{value, synonyms: StringList::from_vec(translations)});
        }

        Ok(EntityDef::new(data, true))
    }
}

/*** NluMap *******************************************************************/

#[derive(Debug)]
pub struct NluMap<M: NluManager + NluManagerStatic + Debug + Send> {
    map: HashMap<LanguageIdentifier, NluState<M>>
}

impl<M: NluManager + NluManagerStatic + Debug + Send> NluMap<M> {
    pub fn new(langs: Vec<LanguageIdentifier>) -> Self {
        let mut managers = HashMap::new();

        // Create a nlu manager per language
        for lang in langs {
            managers.insert(lang.to_owned(), NluState::new(M::new()));
        }

        NluMap{map: managers}
    }

    pub fn get_nlu(&self, lang: &LanguageIdentifier) -> &mut <M as NluManager>::NluType {
        const ERR_MSG: &str = "Received language to the NLU was not registered";
        const NO_NLU_MSG: &str = "received_order can't be called before end_loading";

        self.map.get_mut(&lang).expect(ERR_MSG).nlu.as_mut().expect(NO_NLU_MSG)
    }

    pub fn get_mut(&self, lang: &LanguageIdentifier) -> Result<&mut NluState<M>> {
        let err = || {anyhow!("Received language '{}' has not been registered", lang.to_string())};
        self.map.get_mut(lang).ok_or_else(err)
    }

    pub fn get_mut_nlu_man(&self, lang: &LanguageIdentifier) -> &mut M {
        self.map.get_mut(lang).expect("Language not registered").get_mut_nlu_man()
    }

    pub fn add_intent_to_nlu(
        &mut self,
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
                        self.map.get_mut(lang).expect("Language not registered").get_mut_nlu_man()
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
    
                    self.map.get_mut(lang).expect("Input language was not present before").get_mut_nlu_man()
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
    
}
