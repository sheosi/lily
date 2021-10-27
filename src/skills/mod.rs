#[cfg(feature = "python_skills")]
pub mod local;
mod embedded;
pub mod hermes;

// Standard library
#[cfg(feature = "python_skills")]
use std::{cell::{Ref, RefCell}};
use std::path::{Path, PathBuf};
#[cfg(feature = "python_skills")]
use std::rc::Rc;

// This crate
use crate::exts::LockIt;
use crate::signals::SIG_REG;
use crate::signals::order::dynamic_nlu::DynamicNluRequest;
use self::{embedded::EmbeddedLoader, hermes::HermesLoader};

#[cfg(feature = "python_skills")]
use self::local::LocalLoader;

// Other crates
use anyhow::{anyhow, Result};
use tokio::sync::mpsc::Receiver;
use unic_langid::LanguageIdentifier;

#[cfg(feature="python_skills")]
pub struct RefString {
    current_val: RefCell<Rc<String>>,
    default_val: Rc<String>
}

#[cfg(feature="python_skills")]
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

#[cfg(feature="python_skills")]
fn get_loaders(
    consumer: Receiver<DynamicNluRequest>,
    paths: Vec<PathBuf>
) ->Vec<Box<dyn SkillLoader>> {
    vec![
        Box::new(EmbeddedLoader::new(consumer)),
        Box::new(LocalLoader::new(paths)),
        Box::new(HermesLoader::new())
    ]
}

#[cfg(not(feature="python_skills"))]
fn get_loaders(
    consumer: Receiver<DynamicNluRequest>,
    _paths: Vec<PathBuf>
) ->Vec<Box<dyn SkillLoader>> {
    vec![
        Box::new(EmbeddedLoader::new(consumer)),
        Box::new(HermesLoader::new())
    ]
}

pub fn load_skills(paths: Vec<PathBuf>, curr_langs: &Vec<LanguageIdentifier>, consumer: Receiver<DynamicNluRequest>) -> Result<()> {
    let mut loaders  = get_loaders(consumer, paths);

    for loader in &mut loaders {
        loader.load_skills(curr_langs)?;
    }

    SIG_REG.lock_it().end_load(curr_langs)?;

    Ok(())
}

trait SkillLoader {
    fn load_skills(&mut self,
        langs: &Vec<LanguageIdentifier>) -> Result<()>;
}


