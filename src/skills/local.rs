// Standard library
use std::unimplemented;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

// This crate
use crate::actions::{ActionSet, LocalActionRegistry, PythonAction};
use crate::exts::LockIt;
use crate::python::add_py_folder;
use crate::queries::{ActQuery, LocalQueryRegistry, PythonQuery};
use crate::signals::{collections::{Hook, IntentData}, ActSignal, LocalSignalRegistry, PythonSignal};
use crate::signals::registries::{ACT_REG, QUERY_REG};
use crate::skills::{call_for_skill, Loader};
use crate::nlu::EntityDef;
use crate::vars::{SKILLS_PATH_ERR_MSG, PYTHON_SDK_PATH};

// Other crates
use anyhow::{anyhow, Result};
use pyo3::Python;
use serde::Deserialize;
use thiserror::Error;
use lazy_static::lazy_static;
use log::{error, info, warn};
use unic_langid::LanguageIdentifier;

lazy_static! {
    pub static ref SKILL_PATH: Mutex<HashMap<String, Arc<PathBuf>>> = Mutex::new(HashMap::new());
}

pub struct LocalLoader {
    paths: Vec<PathBuf>
}

impl LocalLoader {
    fn load_package_python(py: Python, path: &Path,
        skill_path: &Path,
        base_sigreg: &LocalSignalRegistry,
        base_actreg: &LocalActionRegistry,
        base_queryreg: &LocalQueryRegistry
    ) -> Result<(LocalSignalRegistry, LocalActionRegistry, LocalQueryRegistry)> {

        let (signal_classes, action_classes, query_classes) = add_py_folder(py, path)?;
        let mut sigreg = base_sigreg.clone();
        let name = skill_path.file_name().ok_or_else(||anyhow!("Got a skill with no name"))?.to_string_lossy();
        match PythonSignal::extend_and_init_classes_py_local(&mut sigreg, py, name.clone().into(), skill_path, signal_classes) {
            Ok(()) =>{Ok(())}
            Err(e) => {
                Err(e.source)
            }
        }?;

        let mut actreg = base_actreg.clone();
        match PythonAction::extend_and_init_classes_local(&mut actreg, py, name.clone().into(), action_classes) {
            Ok(()) => {Ok(())}
            Err(e) => {
                // Also, drop all actions from this package
                sigreg.minus(base_sigreg).remove_from_global();
                Err(e.source)
            }
        }?;

        let mut queryreg = base_queryreg.clone();
        match PythonQuery::extend_and_init_classes_local(&mut queryreg, py,name.clone().into(), query_classes) {
            Ok(()) => {Ok(())}
            Err(e) => {
                // Also, drop all actions from this package
                sigreg.minus(base_sigreg).remove_from_global();
                actreg.minus(base_actreg).remove_from_global();
                Err(e.source)
            }
        }?;

        Ok((sigreg, actreg, queryreg))
    }
}

impl Loader for LocalLoader {

    fn load_skills(&mut self,
        base_sigreg: &LocalSignalRegistry,
        base_actreg: &LocalActionRegistry,
        base_queryreg: &LocalQueryRegistry,
        curr_langs: &Vec<LanguageIdentifier>) -> Result<()>{
        {
            let gil = Python::acquire_gil();
            let py = gil.python();
    
            // Same as load_package_python, but since those actions are crucial we
            // want to make sure they exist and propagate error otherwise
            let path = &PYTHON_SDK_PATH.resolve();
            let _ = add_py_folder(py, path)?;
        }

        let mut not_loaded = vec![];

        let process_skill = |entry: &Path| -> Result<(), HalfBakedDoubleError> {
            let mut skill_sigreg = base_sigreg.clone();
            let mut skill_actreg = base_actreg.clone();
            let mut skill_queryreg = base_queryreg.clone();
            
            let (local_sigreg, local_actreg, local_queryreg)= 
                self.load_code(&entry, &skill_sigreg, &skill_actreg, &skill_queryreg).map_err(|e|{
                    HalfBakedDoubleError::from((skill_sigreg, skill_actreg), e)
                })?;

            skill_sigreg = local_sigreg;
            skill_actreg = local_actreg;
            skill_queryreg = local_queryreg;
            
    
            let skill_name = entry.file_stem().expect("Couldn't get stem from file").to_string_lossy();
            QUERY_REG.lock_it().insert( skill_name.to_string(), skill_queryreg.clone());
            ACT_REG.lock_it().insert( skill_name.to_string(), skill_actreg.clone());
            {
                // Get GIL
                let gil = Python::acquire_gil();
                let py = gil.python();
    
                load_trans(py, &entry, curr_langs).map_err(|e|{
                    HalfBakedDoubleError::from((skill_sigreg.clone(), skill_actreg.clone()), e)
                })?;
            }
    
            load_intents(&mut skill_sigreg, &skill_actreg, &skill_queryreg, &entry, curr_langs).map_err(|e|{
                HalfBakedDoubleError::from((skill_sigreg, skill_actreg), e)
            })?;
            SKILL_PATH.lock_it().insert(skill_name.into(),Arc::new(entry.into()));
            Ok(())
        };
        
        let skl_entries = self.paths.iter()
            .map(|skl_dir|std::fs::read_dir(skl_dir).expect(SKILLS_PATH_ERR_MSG))
            .flatten()
            .filter_map(|r|{match r{
                Ok(v) => Some(v.path()),
                Err(e) => {warn!("Loading an skill failed: {}", e); None}
            }})
            .filter(|p|p.is_dir());
        
    
        for entry in skl_entries {
            match process_skill(&entry) {
                Err(e) => {
                    let (skill_sigreg, skill_actreg) = e.act_sig;
                    skill_sigreg.minus(&base_sigreg).remove_from_global();
                    skill_actreg.minus(&base_actreg).remove_from_global();
                    let skill_name = entry.file_stem().expect("Couldn't get stem from file").to_string_lossy();
                    error!("Skill {} had a problem, won't be available. {}", skill_name, e.source);
                    not_loaded.push(skill_name.into_owned());
                },
                _ => ()
            }
        }
    
        if !not_loaded.is_empty() {
            warn!("Not loaded: {}", not_loaded.join(","));
        }

        Ok(())
    }
}
impl LocalLoader {
    pub fn new(paths: Vec<PathBuf>) -> Self {
        Self{paths}
    }

    fn load_code(&self, skill_path: &Path, base_sigreg: &LocalSignalRegistry, base_actreg: &LocalActionRegistry, base_quereg: &LocalQueryRegistry) -> Result<(LocalSignalRegistry, LocalActionRegistry, LocalQueryRegistry)> {
        // Get GIL
        let gil = Python::acquire_gil();
        let py = gil.python();
        Self::load_package_python(py, &skill_path.join("python"), &skill_path, base_sigreg, base_actreg, base_quereg)
    }
}

/** Thrown after an error while loading classes, 
contains the error and the new classes*/
#[derive(Debug, Error)]
#[error("{source}")]
pub struct HalfBakedDoubleError {
    pub act_sig: (LocalSignalRegistry, LocalActionRegistry),

    pub source: anyhow::Error,
}

impl HalfBakedDoubleError {
    fn from(act_sig:(LocalSignalRegistry, LocalActionRegistry), source: anyhow::Error ) -> Self {
        Self {act_sig, source}
    }
}

fn load_trans(python: Python, skill_path: &Path, curr_langs: &Vec<LanguageIdentifier>) -> Result<()>{
    let lily_py_mod = python.import("lily_ext").map_err(|py_err|anyhow!("Python error while importing lily_ext: {:?}", py_err))?;

    let as_strs: Vec<String> = curr_langs.into_iter().map(|i|i.to_string()).collect();
    call_for_skill(skill_path, |_| lily_py_mod.getattr("__set_translations")?.call((as_strs,), None).map_err(|py_err|anyhow!("Python error while calling __set_translations: {:?}", py_err)))??;

    Ok(())
}

fn load_intents(
    signals: &LocalSignalRegistry,
    actions: &LocalActionRegistry,
    queries: &LocalQueryRegistry,
    path: &Path,
    langs: &Vec<LanguageIdentifier>) -> Result<()> {
    info!("Loading skill: {}", path.to_str().ok_or_else(|| anyhow!("Failed to get the str from path {:?}", path))?);


    let skill_path = Arc::new(path.to_path_buf());
    call_for_skill::<_, Result<()>>(path, |skill_name| {

        let yaml_path = path.join("model.yaml");
        if yaml_path.is_file() {
            #[derive(Debug, Deserialize)]
            #[serde(untagged)]
            enum EventOrAction {
                Event(String),
                Action{action: String}
            }
            #[derive(Debug, Deserialize)]
            struct SkillDef {
                #[serde(flatten)]
                intents: HashMap<String, IntentData>,
                #[serde(default)]
                types: HashMap<String, EntityDef>,
                #[serde(default)]
                events: Vec<EventOrAction>
            }

            // Load Yaml
            let skilldef: SkillDef = {
                let yaml_str = &std::fs::read_to_string(&yaml_path)?;
                serde_yaml::from_str(yaml_str)?
            };

            let sig_order = signals.get_sig_order().expect("Order signal was not initialized");
            for (type_name, data) in skilldef.types.into_iter() {
                sig_order.lock_it().add_slot_type(type_name, data, langs)?;
            }
            
            for (intent_name, data) in skilldef.intents.into_iter() {
                match data.hook.clone() {
                    Hook::Action(name) => {
                        let action = actions.get(&name).ok_or_else(||anyhow!("Action '{}' does not exist", &name))?.lock_it().instance(skill_path.clone());
                        let act_set = ActionSet::create(action);
                        sig_order.lock_it().add_intent(data, &intent_name, &skill_name, act_set, langs)?;
                    },
                    Hook::Query(name) => {
                        // Note: Some very minimal support for queries
                        let q = queries.get(&name).ok_or_else(||anyhow!("Query '{}' does not exist", &name))?;
                        let act_set = ActionSet::create(ActQuery::new(q.clone(), name));
                        sig_order.lock_it().add_intent(data, &intent_name, &skill_name, act_set, langs)?;
                    },
                    Hook::Signal(name) => {
                        // Note: Some very minimal support for signals
                        let s = signals.get(&name).ok_or_else(||anyhow!("Signal '{}' does not exist", &name))?;
                        let act_set = ActionSet::create(ActSignal::new(s.clone(), name));
                        sig_order.lock_it().add_intent(data, &intent_name, &skill_name, act_set, langs)?;
                        unimplemented!();
                    }
                }
                
            }

            let mut def_action = None;
            let mut evs = Vec::new();
            for ev in skilldef.events.into_iter() {
                match ev {
                    EventOrAction::Event(name) => {evs.push(name)},
                    EventOrAction::Action{action} => {def_action = Some(action)}
                }
            }

            if !evs.is_empty() {
                let sigevent = signals.get_sig_event();
                let def_action = def_action.ok_or_else(||anyhow!("Skill contains events but no action linked"))?;
                let action = actions.get(&def_action).ok_or_else(||anyhow!("Action '{}' does not exist", &def_action))?.lock_it().instance(skill_path.clone());
                let act_set = ActionSet::create(action);
                for ev in evs {
                    sigevent.lock().unwrap().add(&ev, act_set.clone());
                }
            }

        }
        Ok(())
    })??;

    Ok(())
}