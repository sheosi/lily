mod embedded;
pub mod hermes;
mod vap;

// Standard library
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

// This crate
use crate::actions::{Action, ACT_REG};
use crate::exts::LockIt;
use crate::queries::{ActQuery, Query};
use crate::nlu::IntentData;
use crate::signals::{ActSignal, SIG_REG, UserSignal};
use crate::signals::order::dynamic_nlu;
use self::{embedded::EmbeddedLoader, hermes::HermesLoader, vap::VapLoader};

// Other crates
use anyhow::Result;
use async_trait::async_trait;
use unic_langid::LanguageIdentifier;

pub fn load_skills(paths: Vec<PathBuf>, curr_langs: &Vec<LanguageIdentifier>) -> Result<()> {
    let mut loaders  = get_loaders(paths);

    for loader in &mut loaders {
        loader.load_skills(curr_langs)?;
    }

    SIG_REG.lock_it().end_load(curr_langs)?;

    Ok(())
}

#[async_trait(?Send)]
pub trait SkillLoader {
    fn load_skills(&mut self,
        langs: &Vec<LanguageIdentifier>) -> Result<()>;
    
    async fn run_loader(&mut self) -> Result<()>;
}


fn add_new_intent(intent_name: String, skill_name: String,
    utts: HashMap<LanguageIdentifier, IntentData>,
    action: Arc<Mutex<dyn Action + Send>>) -> Result<()> {
    
    dynamic_nlu::add_intent(utts, skill_name.clone(), intent_name.clone())?;

    let weak = Arc::downgrade(&action);
    let act_name = action.lock_it().get_name().clone();
    
    ACT_REG.lock_it().insert(&skill_name,&act_name,action)?;

    dynamic_nlu::link_action_intent(intent_name, skill_name, weak)
}


pub fn register_skill(skill_name: &str,
    actions: Vec<(String, HashMap<LanguageIdentifier, IntentData>, Arc<Mutex<dyn Action + Send>>)>,
    signals: Vec<(String, HashMap<LanguageIdentifier, IntentData>, Arc<Mutex<dyn UserSignal + Send>>)>,
    queries: Vec<(String, HashMap<LanguageIdentifier, IntentData>, Arc<Mutex<dyn Query + Send>>)>) -> Result<()> {

    let skill_name_str = skill_name.to_string();
    
    for (intent_name, utts, action) in actions {
        let weak = Arc::downgrade(&action);
        dynamic_nlu::add_intent(utts, skill_name_str.clone(), intent_name.clone())?;
        dynamic_nlu::link_action_intent(
            intent_name, 
            skill_name.to_string(),
            weak
        )?;
    }

    for (name, utts, signal) in signals {
        add_new_intent(name.clone(),
            skill_name_str.clone(),
            utts,
            ActSignal::new(signal, format!("{}_signal_wrapper",name))
        )?;
    }

    for (name, utts , query) in queries {
        add_new_intent(
            name.clone(),
            skill_name_str.clone(),
            utts,
            ActQuery::new(query, format!("{}_query_wrapper",name))
        )?;
    }

    Ok(())
}

fn get_loaders(
    _paths: Vec<PathBuf>
) ->Vec<Box<dyn SkillLoader>> {
    vec![
        Box::new(EmbeddedLoader::new()),
        Box::new(HermesLoader::new()),
        Box::new(VapLoader::new())
    ]
}

