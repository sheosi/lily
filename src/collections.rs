// Standard library
use std::rc::Rc;
use std::cell::{RefCell, RefMut};
use std::clone::Clone;
use std::collections::{HashMap, hash_map::{Entry, IntoIter, IterMut}};
use std::iter::IntoIterator;
use std::fmt;

// Other crates
use anyhow::{anyhow, Result};
use delegate::delegate;

pub struct BaseRegistry<A: ?std::marker::Sized> {
    map: HashMap<String, Rc<RefCell<A>>>
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
        Self{map: self.map.clone()}
    }
}
impl<'a, A: ?std::marker::Sized> IntoIterator for BaseRegistry<A> {
    type Item = (String, Rc<RefCell<A>>);
    type IntoIter = IntoIter<String, Rc<RefCell<A>>>;

    #[inline]
    fn into_iter(self) -> IntoIter<String, Rc<RefCell<A>>> {
        self.map.into_iter()
    }

}
impl<A: ?std::marker::Sized> BaseRegistry<A> {
    pub fn new() -> Self {
        Self{map: HashMap::new()}
    }
    
    pub fn contains(&self, name: &str) -> bool {
        self.map.contains_key(name)
    }

    pub fn iter_mut(&mut self) -> IterMut<String,Rc<RefCell<A>>> {
        self.map.iter_mut()
    }

    pub fn get_map_ref(&mut self) -> &HashMap<String,Rc<RefCell<A>>> {
        &self.map
    }
}

impl<A: ?std::marker::Sized> GlobalReg<A> for BaseRegistry <A> {
    fn remove(&mut self, sig_name: &str) {
        self.map.remove(sig_name);
    }

    fn insert(&mut self, sig_name: String, signal: Rc<RefCell<A>>) -> Result<()> {
        match self.map.entry(sig_name.clone()) {
            Entry::Vacant(v) => {v.insert(signal);Ok(())}
            Entry::Occupied(_) => {Err(anyhow!(format!("Signal {} already exists", sig_name)))}
        }
    }
}

pub trait GlobalReg<A: ?std::marker::Sized> {
    fn remove(&mut self, key: &str);
    fn insert(&mut self, key: String, value: Rc<RefCell<A>>) -> Result<()>;
}

pub struct LocalBaseRegistry<A: ?std::marker::Sized, R: GlobalReg<A> + fmt::Debug> {
    map: HashMap<String, Rc<RefCell<A>>>,
    global_reg: Rc<RefCell<R>>
}

impl<A: ?std::marker::Sized, R: GlobalReg<A> + fmt::Debug> fmt::Debug for LocalBaseRegistry<A,R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let global = self.global_reg.fmt(f);
        f.debug_struct("LocalActionRegistry")
         .field("map", &self.map.keys().collect::<Vec<&String>>())
         .field("global_reg", &global)
         .finish()
    }
}

impl<A: ?std::marker::Sized, R: GlobalReg<A> + fmt::Debug> LocalBaseRegistry<A,R> {
    
    pub fn new(global_reg: Rc<RefCell<R>>) -> Self {
        Self {map: HashMap::new(), global_reg}
    }

    pub fn extend(&mut self, other: Self) {
        self.map.extend(other.map)
    }

    pub fn minus(&self, other: &Self) -> Self {
        let mut res = Self{
            map: HashMap::new(),
            global_reg: self.global_reg.clone()
        };

        for (k,v) in &self.map {
            if !other.map.contains_key(k) {
                res.map.insert(k.clone(), v.clone());
            }
        }

        res
    }

    pub fn remove_from_global(&self) {
        for (sgnl,_) in &self.map {
            self.global_reg.borrow_mut().remove(sgnl);
        }
    }

    pub fn get_global_mut(&self) -> RefMut<R> {
        (*self.global_reg).borrow_mut()
    }

    pub fn insert(&mut self, sig_name: String, signal: Rc<RefCell<A>>) -> Result<()> {
        (*self.global_reg).borrow_mut().insert(sig_name.clone(), signal.clone())?;
        self.map.insert(sig_name, signal);

        Ok(())
    }

    delegate!{to self.map{
        #[call(extend)]
        pub fn extend_with_map(&mut self, other: HashMap<String, Rc<RefCell<A>>>);
        pub fn get(&self, action_name: &str) -> Option<&Rc<RefCell<A>>>;
    }}
}

impl<A: ?std::marker::Sized, R: GlobalReg<A> + fmt::Debug> Clone for LocalBaseRegistry<A,R> {
    fn clone(&self) -> Self {        
        let dup_refs = |pair:(&String, &Rc<RefCell<A>>)| {
            let (key, val) = pair;
            (key.to_owned(), val.clone())
        };

        let new_map: HashMap<String, Rc<RefCell<A>>> = self.map.iter().map(dup_refs).collect();
        Self{map: new_map, global_reg: self.global_reg.clone()}
    }
}