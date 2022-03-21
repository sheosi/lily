// Standard library
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};


// This crate
use crate::actions::{ActionSet, ACT_REG, PythonAction};
use crate::exts::LockIt;
use crate::python::add_py_folder;
use crate::queries::{PythonQuery, QUERY_REG};
use crate::signals::{collections::Hook, PythonSignal, SIG_REG};
use crate::skills::{SkillLoader, register_skill};
use crate::nlu::{EntityData, EntityDef, IntentData, OrderKind, SlotData};
use crate::python::{call_for_skill,try_translate, try_translate_all};
use crate::vars::{SKILLS_PATH_ERR_MSG, PYTHON_SDK_PATH};

// Other crates
use anyhow::{anyhow, Result};
use pyo3::Python;
use serde::{Deserialize, Deserializer, de::{self, SeqAccess, Visitor}, Serialize, Serializer, ser::SerializeSeq};
use thiserror::Error;
use lazy_static::lazy_static;
use lily_common::other::false_val;
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
        skill_name: &str,
        skill_path: &Path,
    ) -> Result<(Vec<String>, Vec<String>, Vec<String>)> {

        let (signal_classes, action_classes, query_classes) = add_py_folder(py, path)?;
        let name = skill_path.file_name().ok_or_else(||anyhow!("Got a skill with no name"))?.to_string_lossy();
        let sigs = PythonSignal::extend_and_init_classes_py(py, name.clone().into(), skill_path, signal_classes)
            .map_err(|e|Err(e.source))?;

        let acts= PythonAction::extend_and_init_classes(py, name.clone().into(), action_classes, Arc::new(skill_path.to_path_buf()))
            .map_err(|e|Err(e.source))?;

        let queries = PythonQuery::extend_and_init_classes(py,name.clone().into(), query_classes)
            .map_err(|e|Err(e.source))?;

        Ok((sigs, acts, queries))
    }
}

impl SkillLoader for LocalLoader {

    fn load_skills(&mut self,
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
            let skill_name = entry.file_stem().expect("Couldn't get stem from file").to_string_lossy().to_string();

            let (sigs, acts, queries) = {
                // Get GIL
                let gil = Python::acquire_gil();
                let py = gil.python();
                Self::load_package_python(py, &entry.join("python"), &skill_name, &entry).map_err(|e|
                    HalfBakedDoubleError::new(vec![], vec![], vec![], e)
                )?
            };
            
            {
                // Get GIL
                let gil = Python::acquire_gil();
                let py = gil.python();
    
                load_trans(py, entry, curr_langs).map_err(|e|{
                    HalfBakedDoubleError::new(sigs.clone(), acts.clone(), queries.clone(), e)
                })?;
            }
    
            load_intents(&entry, curr_langs).map_err(|e| {
                HalfBakedDoubleError::new(sigs, acts, queries, e)
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
                    let skill_name = entry.file_stem().expect("Couldn't get stem from file").to_string_lossy().to_string();
                    SIG_REG.lock_it().remove_several(&skill_name, &e.signals)?;
                    ACT_REG.lock_it().remove_several(&skill_name, &e.acts)?;
                    
                    error!("Skill {} had a problem, won't be available. {}", skill_name, e.source);
                    not_loaded.push(skill_name);
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
}

#[derive(Clone, Debug, Deserialize)]
pub struct YamlSlotData {
    #[serde(rename="type")]
    pub slot_type: YamlOrderKind,
    #[serde(default="false_val")]
    pub required: bool,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub reprompt: Option<String>
}

#[derive(Clone, Debug, Deserialize)]
pub struct YamlIntentData {

    #[serde(alias = "samples", alias = "sample")]
    pub utts:  StringList,
    #[serde(default)]
    pub slots: HashMap<String, YamlSlotData>,
    #[serde(flatten)]
    pub hook: Hook
}

impl YamlIntentData {
    pub fn try_into_with_trans(self, lang: &LanguageIdentifier) -> Result<IntentData> {
        let t  = self.utts.into_translation(lang).map_err(|v|anyhow!("Translation failed for: {:?}", v))?;
        let utts = StringList::from_vec(t);
        let lang_str = &lang.to_string();
        let mut slots = HashMap::new();

        for (k, v) in self.slots.into_iter() {
            slots.insert(k, SlotData {
                slot_type: match v.slot_type {
                    YamlOrderKind::Ref(t) => OrderKind::Ref(t),
                    YamlOrderKind::Def(t) => OrderKind::Def(t.try_into_with_trans(lang)?),
                },
                required: v.required,
                prompt: v.prompt.as_ref().map(|p|try_translate(&p, lang_str)).transpose()?,
                reprompt: v.reprompt.as_ref().map(|p|try_translate(&p, lang_str)).transpose()?
            });
        }
    
        Ok(IntentData{utts, slots, hook: self.hook})
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum YamlOrderKind {
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
            // Only first translation
            let mut translations = try_translate_all(&trans_data, &lang.to_string())?;
            let value = translations.swap_remove(0);
            
            data.push(EntityData{value, synonyms: StringList::from_vec(translations)});
        }

        Ok(EntityDef::new(data, true))
    }
}

fn load_trans(python: Python, skill_path: &Path, curr_langs: &Vec<LanguageIdentifier>) -> Result<()>{
    let lily_py_mod = python.import("lily_ext").map_err(|py_err|anyhow!("Python error while importing lily_ext: {:?}", py_err))?;

    let as_strs: Vec<String> = curr_langs.into_iter().map(|i|i.to_string()).collect();
    call_for_skill(skill_path, |_| lily_py_mod.getattr("__set_translations")?.call((as_strs,), None).map_err(|py_err|anyhow!("Python error while calling __set_translations: {:?}", py_err)))??;

    Ok(())
}

fn load_intents(
    path: &Path,
    langs: &Vec<LanguageIdentifier>) -> Result<()> {
    info!("Loading skill: {}", path.to_str().ok_or_else(|| anyhow!("Failed to get the str from path {:?}", path))?);

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
                intents: HashMap<String, YamlIntentData>,
                #[serde(default)]
                types: HashMap<String, YamlEntityDef>,
                #[serde(default)]
                events: Vec<EventOrAction>
            }

            // Load Yaml
            let skilldef: SkillDef = {
                let yaml_str = &std::fs::read_to_string(&yaml_path)?;
                serde_yaml::from_str(yaml_str)?
            };

            
            let sig_grd = SIG_REG.lock_it();
            let sig_order = sig_grd.get_sig_order().expect("Order signal was not initialized");
            for (type_name, data) in skilldef.types.into_iter() {
                for lang in langs {
                    sig_order.lock_it().add_slot_type(type_name.clone(), data.clone().try_into_with_trans(lang)?, lang);
                }
            }
            
            for (intent_name, data) in skilldef.intents.into_iter() {
                let intent_trans = langs.into_iter().map(|l|{
                    Ok((l, data.clone().try_into_with_trans(l)?))
                }).collect::<Result<_>>()?;
                
                register_skill();
                match data.hook.clone() {
                    Hook::Action(name) => {
                        let action_grd = ACT_REG.lock_it();
                        let action = action_grd.get(&skill_name,&name).ok_or_else(||anyhow!("Action '{}' does not exist", &name))?;

                        sig_order.lock_it().add_intent_action(
                            intent_trans,
                            &intent_name,
                            &skill_name,
                            action
                        )?;
                        
                    },
                    Hook::Query(name) => {
                        // Note: Some very minimal support for queries
                        let query_grd = QUERY_REG.lock_it();
                        let q = query_grd.get(&skill_name, &name).ok_or_else(||anyhow!("Query '{}' does not exist", &name))?;

                        sig_order.lock_it().add_intent_query(
                            intent_trans, 
                            &skill_name,
                            &intent_name,
                            name,
                            q.to_owned()
                        )?;

                    },
                    Hook::Signal(name) => {
                        // Note: Some very minimal support for signals
                        let s = sig_grd.get(&skill_name, &name).ok_or_else(||anyhow!("Signal '{}' does not exist", &name))?;
                        
                        sig_order.lock_it().add_intent_signal(
                            intent_trans,
                            &intent_name,
                            &skill_name,
                            name,
                            s.clone()
                        )?;
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
                let sigevent = sig_grd.get_sig_event();
                let def_action = def_action.ok_or_else(||anyhow!("Skill contains events but no action linked"))?;
                let act_grd = ACT_REG.lock_it();
                let action = act_grd.get(&skill_name, &def_action).ok_or_else(||anyhow!("Action '{}' does not exist", &def_action))?;
                let act_set = ActionSet::create(Arc::downgrade(action));

                for ev in evs {
                    sigevent.lock().unwrap().add(&ev, act_set.clone());
                }
            }

        }
        Ok(())
    })??;

    Ok(())
}

/** Thrown after an error while loading classes, 
contains the error and the new classes*/
#[derive(Debug, Error)]
#[error("{source}")]
pub struct HalfBakedDoubleError {
    pub acts: Vec<String>,
    pub queries: Vec<String>,
    pub signals: Vec<String>,

    pub source: anyhow::Error,
}

impl HalfBakedDoubleError {
    fn new(acts: Vec<String>, queries: Vec<String>, signals: Vec<String>, source: anyhow::Error ) -> Self {
        Self {acts, queries, signals, source}
    }
}

#[derive(Clone, Debug)]
pub struct StringList {
    pub data: Vec<String>
}

impl StringList {
    pub fn new() -> Self {
        Self{data: Vec::new()}
    }
    pub fn from_vec(vec: Vec<String>) -> Self {
        Self{ data: vec}
    }

    /// Returns an aggregated vector with the translations of all entries
    pub fn into_translation(self, lang: &LanguageIdentifier) -> Result<Vec<String>,Vec<String>> {
        let lang_str = lang.to_string();

        let (utts_data, failed):(Vec<_>,Vec<_>) = self.data.into_iter()
        .map(|utt|try_translate_all(&utt, &lang_str))
        .partition(Result::is_ok);

        if failed.is_empty() {
            let utts = utts_data.into_iter().map(Result::unwrap)
            .flatten().collect();

            Ok(utts)
        }
        else {
            let failed = failed.into_iter().map(Result::unwrap)
            .flatten().collect();
            Err(failed)            
        }
    }

    /// Returns an aggregated vector with the translations of all entries
    pub fn to_translation(&self, lang: &LanguageIdentifier) -> Result<Vec<String>,Vec<String>> {
        let lang_str = lang.to_string();

        let (utts_data, failed):(Vec<_>,Vec<_>) = self.data.iter()
        .map(|utt|try_translate_all(&utt, &lang_str))
        .partition(Result::is_ok);

        if failed.is_empty() {
            let utts = utts_data.into_iter().map(Result::unwrap)
            .flatten().collect();

            Ok(utts)
        }
        else {
            let failed = failed.into_iter().map(Result::unwrap)
            .flatten().collect();
            Err(failed)            
        }
    }
}

struct StringListVisitor;

impl<'de> Visitor<'de> for StringListVisitor {
    type Value = StringList;
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("either a string or a list containing strings")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E> where E: de::Error{
        Ok(StringList{data:vec![v.to_string()]})
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E> where E: de::Error{
        Ok(StringList{data:vec![v]})
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error> where A: SeqAccess<'de> {
        let mut res = StringList{data: Vec::with_capacity(seq.size_hint().unwrap_or(1))};
        loop {
            match seq.next_element()? {
                Some(val) => {res.data.push(val)},
                None => {break}
            }
        }

        return Ok(res);   
    }
}

impl<'de> Deserialize<'de> for StringList {
    fn deserialize<D>(deserializer: D) -> Result<StringList, D::Error>
    where D: Deserializer<'de> {
        deserializer.deserialize_any(StringListVisitor)
    }
}

// Serialize this as a list of strings
impl Serialize for StringList {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,{
        let mut seq = serializer.serialize_seq(Some(self.data.len()))?;
        for e in &self.data {
            seq.serialize_element(e)?;
        }
        seq.end()
    }
}