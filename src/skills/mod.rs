#[cfg(feature = "python_skills")]
pub mod local;
mod embedded;
pub mod hermes;

// Standard library
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

// This crate
use crate::actions::{Action, ACT_REG};
use crate::exts::LockIt;
use crate::queries::{ActQuery, Query};
use crate::signals::{ActSignal, SIG_REG, Signal, UserSignal};
use crate::signals::order::dynamic_nlu;
use self::{embedded::EmbeddedLoader, hermes::HermesLoader};

#[cfg(feature = "python_skills")]
use self::local::LocalLoader;

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


fn link_signal_intent(intent_name: String, skill_name: String, signal_name: String,
    signal: Arc<Mutex<dyn UserSignal + Send>>) -> Result<()> {

    let arc = ActSignal::new(signal, signal_name);
    let weak = Arc::downgrade(&arc);
    
    ACT_REG.lock_it().insert(
        &skill_name,
        &format!("{}_signal_wrapper",intent_name),
        arc
    )?;

    dynamic_nlu::link_action_intent(intent_name, skill_name, weak)
}

// Note: Some very minimal support for queries
fn link_query_intent(intent_name: String, skill_name: String,
    query_name: String, query: Arc<Mutex<dyn Query + Send>>) -> Result<()> {
    
    let arc = ActQuery::new(query, query_name);
    let weak = Arc::downgrade(&arc);
    ACT_REG.lock_it().insert(
        &skill_name,
        &format!("{}_query_wrapper",intent_name),
        arc
    )?;

    dynamic_nlu::link_action_intent(intent_name, skill_name, weak)
}


pub fn register_skill(skill_name: &str,
    actions: HashMap<String, (HashMap<LanguageIdentifier, IntentData>, Arc<Mutex<dyn Action + Send>>)>,
    signals: HashMap<String, (HashMap<LanguageIdentifier, IntentData>, Arc<Mutex<dyn UserSignal + Send>>)>,
    queries: HashMap<String, (HashMap<LanguageIdentifier, IntentData>, Arc<Mutex<dyn Query + Send>>)>) -> Result<()> {
    
    // TODO! add utterances
    for (name, (utts, action)) in actions {
        let weak = Arc::downgrade(&action);
        dynamic_nlu::link_action_intent(
            name, 
            skill_name.to_string(),
            weak)?;
        
        add_intent(
            skill_name.into(),
            name,
            utts,
        )?;
    }

    for (name, (utts, signal)) in signals {
        link_signal_intent(name, skill_name.into(), "TODO!".into(), signal);

        add_intent(
            skill_name.into(),
            name,
            utts,
        )?;
    }

    for (name, (utts , query)) in queries {
        link_query_intent(name, skill_name.into(), "TODO!".into(), query);

        add_intent(
            skill_name.into(),
            name,
            utts,
        )?;
    }

    Ok(())
}

#[cfg(feature="python_skills")]
fn get_loaders(
    paths: Vec<PathBuf>
) ->Vec<Box<dyn SkillLoader>> {
    vec![
        Box::new(EmbeddedLoader::new()),
        Box::new(LocalLoader::new(paths)),
        Box::new(HermesLoader::new()),
        //Box::new(VapLoader::new())
    ]
}

#[cfg(not(feature="python_skills"))]
fn get_loaders(
    _paths: Vec<PathBuf>
) ->Vec<Box<dyn SkillLoader>> {
    vec![
        Box::new(EmbeddedLoader::new()),
        Box::new(HermesLoader::new()),
        //Box::new(VapLoader::new())
    ]
}

