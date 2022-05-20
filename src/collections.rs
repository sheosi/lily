// Standard library
use std::clone::Clone;
use std::collections::{
    hash_map::{Entry, IntoIter, IterMut},
    HashMap,
};
use std::fmt;
use std::iter::IntoIterator;
use std::sync::{Arc, Mutex};

// This crate
use crate::vars::mangle;

// Other crates
use anyhow::{anyhow, Result};
use delegate::delegate;

// Elements are stored as __skill_element, joining both into one key
pub struct BaseRegistry<A: ?std::marker::Sized> {
    map: HashMap<String, Arc<Mutex<A>>>,
}

impl<A: ?std::marker::Sized> fmt::Debug for BaseRegistry<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BaseRegistry")
            .field("map", &self.map.keys().collect::<Vec<&String>>())
            .finish()
    }
}
impl<A: ?std::marker::Sized> Clone for BaseRegistry<A> {
    fn clone(&self) -> Self {
        Self {
            map: self.map.clone(),
        }
    }
}
impl<'a, A: ?std::marker::Sized> IntoIterator for BaseRegistry<A> {
    type Item = (String, Arc<Mutex<A>>);
    type IntoIter = IntoIter<String, Arc<Mutex<A>>>;

    #[inline]
    fn into_iter(self) -> IntoIter<String, Arc<Mutex<A>>> {
        self.map.into_iter()
    }
}
impl<A: ?std::marker::Sized> BaseRegistry<A> {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub fn get_map_ref(&mut self) -> &HashMap<String, Arc<Mutex<A>>> {
        &self.map
    }

    pub fn get<'a>(&'a self, skill_name: &str, item: &str) -> Option<&'a Arc<Mutex<A>>> {
        let mangled = mangle(skill_name, item);
        self.map.get(&mangled)
    }

    pub fn insert(&mut self, skill_name: &str, name: &str, object: Arc<Mutex<A>>) -> Result<()> {
        let mangled = mangle(skill_name, name);
        match self.map.entry(mangled) {
            Entry::Vacant(v) => {
                v.insert(object);
                Ok(())
            }
            Entry::Occupied(_) => Err(anyhow!(format!("{}: {} already exists", skill_name, name))),
        }
    }

    pub fn remove(&mut self, skill_name: &str, name: &str) -> Result<()> {
        let mangled = mangle(skill_name, name);
        match self.map.remove(&mangled) {
            Some(_) => Ok(()),
            None => Err(anyhow!(format!("{}: {} does not exist", skill_name, name))),
        }
    }

    delegate! {to self.map {
        pub fn iter_mut(&mut self) -> IterMut<String,Arc<Mutex<A>>>;
        #[call(remove)]
        pub fn remove_mangled(&mut self, name: &str) -> Option<Arc<Mutex<A>>>;
    }}
}
