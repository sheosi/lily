// Standard library
use std::{cell::{Ref, RefCell}, unimplemented};
use std::collections::HashMap;
use std::mem::replace;
use std::path::Path;
use std::rc::Rc;
use std::sync::{Arc, mpsc::Receiver};

// This crate
use crate::actions::{ActionSet, ActionRegistry, ActionRegistryShared, LocalActionRegistry, PythonAction};
use crate::collections::GlobalReg;
use crate::python::add_py_folder;
use crate::queries::{LocalQueryRegistry, PythonQuery, QueryRegistry};
use crate::signals::{Hook, IntentData, LocalSignalRegistry, new_signal_order, PythonSignal, SignalRegistry, SignalRegistryShared, Timer, PollQuery};
use crate::signals::order::EntityAddValueRequest;
use crate::nlu::EntityDef;
use crate::vars::{SKILLS_PATH_ERR_MSG, PYTHON_SDK_PATH};

// Other crates
use anyhow::{anyhow, Result};
use pyo3::Python;
use serde::Deserialize;
use thiserror::Error;
use log::{error, info, warn};
use unic_langid::LanguageIdentifier;

thread_local! {
    pub static PYTHON_LILY_SKILL: RefString = RefString::new("<None>");
}

trait IntoMapping {
    fn into_mapping(self) -> Option<serde_yaml::Mapping>;
}

impl IntoMapping for serde_yaml::Value {
    fn into_mapping(self) -> Option<serde_yaml::Mapping> {
        match self {
            serde_yaml::Value::Mapping(mapping) => Some(mapping),
            _ => None
        }
    }
}

pub fn call_for_skill<F, R>(path: &Path, f: F) -> Result<R> where F: FnOnce(Rc<String>) -> R {
    let canon_path = path.canonicalize()?;
    let skill_name = extract_name(&canon_path)?;
    std::env::set_current_dir(&canon_path)?;
    PYTHON_LILY_SKILL.with(|c| c.set(skill_name.clone()));
    let r = f(skill_name);
    PYTHON_LILY_SKILL.with(|c| c.clear());

    Ok(r)
}

pub struct RefString {
    current_val: RefCell<Rc<String>>,
    default_val: Rc<String>
}

impl RefString {
    pub fn new(def: &str) -> Self {
        let default_val = Rc::new(def.to_owned());
        Self {current_val: RefCell::new(default_val.clone()), default_val}
    }

    pub fn clear(&self) {
        self.current_val.replace(self.default_val.clone());
    }

    pub fn set(&self, val: Rc<String>) {
        self.current_val.replace(val);
    }

    pub fn borrow(&self) -> Ref<String> {
        Ref::map(self.current_val.borrow(), |r|r.as_ref())
    }
}

fn extract_name(path: &Path) -> Result<Rc<String>> {
    let os_str = path.file_name().ok_or_else(||anyhow!("Can't get skill path's name"))?;
    let skill_name_str = os_str.to_str().ok_or_else(||anyhow!("Can't transform skill path name to str"))?;
    Ok(Rc::new(skill_name_str.to_string()))
}

fn load_trans(python: Python, skill_path: &Path, curr_langs: &Vec<LanguageIdentifier>) -> Result<()>{
    let lily_py_mod = python.import("lily_ext").map_err(|py_err|anyhow!("Python error while importing lily_ext: {:?}", py_err))?;

    let as_strs: Vec<String> = curr_langs.into_iter().map(|i|i.to_string()).collect();
    call_for_skill(skill_path, |_| lily_py_mod.call("__set_translations", (as_strs,), None).map_err(|py_err|anyhow!("Python error while calling __set_translations: {:?}", py_err)))??;

    Ok(())
}

pub fn load_intents(
    signals: &LocalSignalRegistry,
    actions: &LocalActionRegistry,
    queries: &LocalQueryRegistry,
    path: &Path) -> Result<()> {
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
                sig_order.borrow_mut().add_slot_type(type_name, data)?;
            }
            
            for (intent_name, data) in skilldef.intents.into_iter() {
                match data.hook.clone() {
                    Hook::Action(name) => {
                        let action = actions.get(&name).ok_or_else(||anyhow!("Action '{}' does not exist", &name))?.borrow_mut().instance(skill_path.clone());
                        let act_set = ActionSet::create();
                        act_set.lock().unwrap().add_action(action)?;
                        sig_order.borrow_mut().add_intent(data, &intent_name, &skill_name, act_set)?;
                    },
                    Hook::Query(name) => {
                        // TODO: Support queries
                        let q = queries.get(&name).ok_or_else(||anyhow!("Query '{}' does not exist", &name))?;
                        unimplemented!();
                    },
                    Hook::Signal(name) => {
                        // TODO: Support signals
                        let s = signals.get(&name).ok_or_else(||anyhow!("Signal '{}' does not exist", &name))?;
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
                let action = actions.get(&def_action).ok_or_else(||anyhow!("Action '{}' does not exist", &def_action))?.borrow_mut().instance(skill_path.clone());
                let act_set = ActionSet::create();
                act_set.lock().unwrap().add_action(action)?;
                for ev in evs {
                    sigevent.lock().unwrap().add(&ev, act_set.clone());
                }
            }

        }
        Ok(())
    })??;

    Ok(())
}


/** Used by other modules, launched after an error while loading classes, 
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


pub fn load_skills(path: &Path, curr_lang: &Vec<LanguageIdentifier>, consumer: Receiver<EntityAddValueRequest>) -> Result<SignalRegistry> {
    let mut loaders: Vec<Box<dyn Loader>> = vec![
        Box::new(PythonLoader::new()),
        Box::new(EmbeddedLoader::new(consumer))
    ];

    let global_sigreg = Rc::new(RefCell::new(SignalRegistry::new()));
    let base_sigreg = LocalSignalRegistry::new(global_sigreg.clone());

    let global_actreg = Rc::new(RefCell::new(ActionRegistry::new()));
    let base_actreg = LocalActionRegistry::new(global_actreg.clone());

    let global_queryreg = Rc::new(RefCell::new(QueryRegistry::new()));
    let base_queryreg = LocalQueryRegistry::new(global_queryreg.clone());

    for loader in &mut loaders {
        loader.init_base(global_sigreg.clone(), global_actreg.clone(), curr_lang.to_owned())?;
    }

    let mut not_loaded = vec![];

    let process_skill = |entry: &Path| -> Result<(), HalfBakedDoubleError> {
        let mut skill_sigreg = base_sigreg.clone();
        let mut skill_actreg = base_actreg.clone();
        let mut skill_queryreg = base_queryreg.clone();
        for loader in &loaders {
            let (local_sigreg, local_actreg, local_queryreg)= 
                loader.load_code(&entry, &skill_sigreg, &skill_actreg, &skill_queryreg).map_err(|e|{
                    HalfBakedDoubleError::from((skill_sigreg, skill_actreg), e)
                })?;

                skill_sigreg = local_sigreg;
                skill_actreg = local_actreg;
                skill_queryreg = local_queryreg;
        }
        {
            // Get GIL
            let gil = Python::acquire_gil();
            let py = gil.python();

            load_trans(py, &entry, curr_lang).map_err(|e|{
                HalfBakedDoubleError::from((skill_sigreg.clone(), skill_actreg.clone()), e)
            })?;
        }

        load_intents(&mut skill_sigreg, &skill_actreg, &skill_queryreg, &entry).map_err(|e|{
            HalfBakedDoubleError::from((skill_sigreg, skill_actreg), e)
        })?;
        Ok(())
    };

    for entry in std::fs::read_dir(path).expect(SKILLS_PATH_ERR_MSG) {
        let entry = entry?.path();
        if entry.is_dir() {
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
    }

    if !not_loaded.is_empty() {
        warn!("Not loaded: {}", not_loaded.join(","));
    }

    global_sigreg.borrow_mut().end_load(curr_lang)?;

    // This is overall stupid but haven't found any other (interesting way to do it)
    // We need the variable to help lifetime analisys
    let res = global_sigreg.borrow_mut().clone();
    Ok(res)
}

trait Loader {
    fn init_base(&mut self, glob_sigreg: SignalRegistryShared, glob_actreg: ActionRegistryShared, lang: Vec<LanguageIdentifier>) -> Result<()>;
    fn load_code(&self, skill_path: &Path, base_sigreg: &LocalSignalRegistry, base_actreg: &LocalActionRegistry, base_quereg: &LocalQueryRegistry) -> Result<(LocalSignalRegistry, LocalActionRegistry, LocalQueryRegistry)> ;
}

struct PythonLoader {}

impl PythonLoader {
    pub fn load_package_python(py: Python, path: &Path,
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
        match PythonAction::extend_and_init_classes_local(&mut actreg, py, name.into(), action_classes) {
            Ok(()) => {Ok(())}
            Err(e) => {
                // Also, drop all actions from this package
                sigreg.minus(base_sigreg).remove_from_global();
                Err(e.source)
            }
        }?;

        let mut queryreg = base_queryreg.clone();
        match PythonQuery::extend_and_init_classes_local(&mut queryreg, py, query_classes) {
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

impl Loader for PythonLoader {
    fn init_base(&mut self, _glob_sigreg: SignalRegistryShared, _glob_actreg: ActionRegistryShared, _lang: Vec<LanguageIdentifier>) -> Result<()> {
        // Get GIL
        let gil = Python::acquire_gil();
        let py = gil.python();

        // Same as load_package_python, but since those actions are crucial we
        // want to make sure they exist and propagate error otherwise
        let path = &PYTHON_SDK_PATH.resolve();
        let _ = add_py_folder(py, path)?;
        //PythonSignal::extend_and_init_classes_py_local(&mut sigreg, py, path, initial_signals)?;
        //PythonAction::extend_and_init_classes_local(&mut actreg, py, initial_actions)?;

        Ok(())
    }

    fn load_code(&self, skill_path: &Path, base_sigreg: &LocalSignalRegistry, base_actreg: &LocalActionRegistry, base_quereg: &LocalQueryRegistry) -> Result<(LocalSignalRegistry, LocalActionRegistry, LocalQueryRegistry)> {
        // Get GIL
        let gil = Python::acquire_gil();
        let py = gil.python();
        Self::load_package_python(py, &skill_path.join("python"), &skill_path, base_sigreg, base_actreg, base_quereg)
    }
}
impl PythonLoader {
    fn new() -> Self {
        Self{}
    }
}

struct EmbeddedLoader {
    consumer: Option<Receiver<EntityAddValueRequest>>
}

impl EmbeddedLoader {
    fn new(consumer: Receiver<EntityAddValueRequest>) -> Self {
        Self{consumer: Some(consumer)}
    }
}

impl Loader for EmbeddedLoader {
    fn init_base(&mut self, glob_sigreg: SignalRegistryShared, _glob_actreg: ActionRegistryShared, lang: Vec<LanguageIdentifier>) -> Result<()> {
        let mut mut_sigreg = glob_sigreg.borrow_mut();
        let consumer = replace(&mut self.consumer, None).expect("Consumer already consumed");
        mut_sigreg.set_order(Rc::new(RefCell::new(new_signal_order(lang, consumer))))?;
        mut_sigreg.insert("embedded".into(), "private__poll_query".into(), Rc::new(RefCell::new(PollQuery::new())))?;
        mut_sigreg.insert("embedded".into(),"timer".into(), Rc::new(RefCell::new(Timer::new())))?;

        Ok(())
    }

    fn load_code(&self, _skill_path: &Path, base_sigreg: &LocalSignalRegistry, base_actreg: &LocalActionRegistry, base_quereg: &LocalQueryRegistry) -> Result<(LocalSignalRegistry, LocalActionRegistry, LocalQueryRegistry)> {
        Ok((base_sigreg.clone(), base_actreg.clone(), base_quereg.clone()))
    }
}