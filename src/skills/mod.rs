#[cfg(feature = "python_skills")]
pub mod local;
mod embedded;
pub mod hermes;

// Standard library
use std::{cell::{Ref, RefCell}};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

// This crate
use crate::actions::{ActionRegistry, LocalActionRegistry};
use crate::exts::LockIt;
use crate::queries::{LocalQueryRegistry, QueryRegistry};
use crate::signals::{LocalSignalRegistry, SignalRegistry};
use crate::signals::order::dynamic_nlu::EntityAddValueRequest;
use self::{embedded::EmbeddedLoader, hermes::HermesLoader};

#[cfg(feature = "python_skills")]
use self::local::LocalLoader;

// Other crates
use anyhow::{anyhow, Result};
use tokio::sync::mpsc::Receiver;
use unic_langid::LanguageIdentifier;

thread_local! {
    pub static PYTHON_LILY_SKILL: RefString = RefString::new("<None>");
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

#[cfg(feature="python_skills")]
fn get_loaders(
    consumer: Receiver<EntityAddValueRequest>,
    paths: Vec<PathBuf>
) ->Vec<Box<dyn Loader>> {
    vec![
        Box::new(EmbeddedLoader::new(consumer)),
        Box::new(LocalLoader::new(paths)),
        Box::new(HermesLoader::new())
    ]
}

#[cfg(not(feature="python_skills"))]
fn get_loaders(
    consumer: Receiver<EntityAddValueRequest>,
    paths: Vec<PathBuf>
) ->Vec<Box<dyn Loader>> {
    vec![
        Box::new(EmbeddedLoader::new(consumer)),
        Box::new(HermesLoader::new())
    ]
}

pub fn load_skills(paths: Vec<PathBuf>, curr_langs: &Vec<LanguageIdentifier>, consumer: Receiver<EntityAddValueRequest>) -> Result<SignalRegistry> {
    let mut loaders  = get_loaders(consumer, paths);

    let global_sigreg = Arc::new(Mutex::new(SignalRegistry::new()));
    let base_sigreg = LocalSignalRegistry::new(global_sigreg.clone());

    let global_actreg = Arc::new(Mutex::new(ActionRegistry::new()));
    let base_actreg = LocalActionRegistry::new(global_actreg.clone());

    let global_queryreg = Arc::new(Mutex::new(QueryRegistry::new()));
    let base_queryreg = LocalQueryRegistry::new(global_queryreg.clone());

    for loader in &mut loaders {
        loader.load_skills(&base_sigreg, &base_actreg, &base_queryreg, curr_langs)?;
    }

    global_sigreg.lock_it().end_load(curr_langs)?;

    // This is overall stupid but haven't found any other (interesting way to do it)
    // We need the variable to help lifetime analisys
    let res = global_sigreg.lock_it().clone();
    Ok(res)
}

trait Loader {
    fn load_skills(&mut self,
        base_sigreg: &LocalSignalRegistry,
        base_actreg: &LocalActionRegistry,
        base_queryreg: &LocalQueryRegistry,
        langs: &Vec<LanguageIdentifier>) -> Result<()>;
}


