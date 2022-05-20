/**
 * Collections for the Order Signal.
 *
 * Take into account that the methods here expect the translation to be done
 * already, SkillLoader is the one responsible for that (either directly or
 * expecting the translation to be done already).
 */
// Standard library
use std::collections::HashMap;
use std::fmt::Debug;

// This crate
use crate::nlu::{IntentData, NluManager, NluManagerStatic, OrderKind};
use crate::signals::order::NluState;
use crate::vars::mangle;

// Other crates
use anyhow::{anyhow, Result};
use serde::Deserialize;
use unic_langid::LanguageIdentifier;

/*** Config ********************************************************************/
#[derive(Clone, Debug, Deserialize)]
pub enum Hook {
    #[serde(rename = "query")]
    Query(String),
    #[serde(rename = "action")]
    Action(String),
    #[serde(rename = "signal")]
    Signal(String),
}

/*** NluMap *******************************************************************/

#[derive(Debug)]
pub struct NluMap<M: NluManager + NluManagerStatic + Debug + Send> {
    map: HashMap<LanguageIdentifier, NluState<M>>,
}

impl<M: NluManager + NluManagerStatic + Debug + Send> NluMap<M> {
    pub fn new(langs: Vec<LanguageIdentifier>) -> Self {
        let mut managers = HashMap::new();

        // Create a nlu manager per language
        for lang in langs {
            managers.insert(lang.to_owned(), NluState::new(M::new()));
        }

        NluMap { map: managers }
    }

    pub fn get_nlu(&mut self, lang: &LanguageIdentifier) -> &mut <M as NluManager>::NluType {
        const ERR_MSG: &str = "Received language to the NLU was not registered";
        const NO_NLU_MSG: &str = "received_order can't be called before end_loading";

        self.map
            .get_mut(&lang)
            .expect(ERR_MSG)
            .nlu
            .as_mut()
            .expect(NO_NLU_MSG)
    }

    pub fn get_mut(&mut self, lang: &LanguageIdentifier) -> Result<&mut NluState<M>> {
        let err = || {
            anyhow!(
                "Received language '{}' has not been registered",
                lang.to_string()
            )
        };
        self.map.get_mut(lang).ok_or_else(err)
    }

    pub fn get_mut_nlu_man(&mut self, lang: &LanguageIdentifier) -> &mut M {
        self.map
            .get_mut(lang)
            .expect("Language not registered")
            .get_mut_nlu_man()
    }

    pub fn add_intent_to_nlu(
        &mut self,
        sig_arg: IntentData,
        intent_name: &str,
        skill_name: &str,
        lang: &LanguageIdentifier,
    ) -> Result<()> {
        //First, register all slots

        for (slot_name, slot_data) in sig_arg.slots.iter() {
            // Handle that slot types might be defined on the spot
            if let OrderKind::Def(def) = slot_data.slot_type.clone() {
                let name = mangle(skill_name, slot_name);
                self.map
                    .get_mut(lang)
                    .expect("Language not registered")
                    .get_mut_nlu_man()
                    .add_entity(name.clone(), def.clone());
            }
        }

        self.map
            .get_mut(lang)
            .expect("Input language was not present before")
            .get_mut_nlu_man()
            .add_intent(intent_name, sig_arg.into_utterances(skill_name));

        Ok(())
    }
}
