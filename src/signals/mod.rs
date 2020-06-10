mod order;

pub use self::order::*;

// Standard library
use std::rc::Rc;
use std::cell::RefCell;
use std::collections::HashMap;

// This crate
use crate::actions::ActionSet;

// Other crates
use anyhow::Result;
use cpython::{PyDict, Python};

// A especial signal to be called by the system whenever something happens
pub struct SignalEvent {
    event_map: OrderMap
}

impl SignalEvent {
    pub fn new() -> Self {
        Self {event_map: OrderMap::new()}
    }

    pub fn add(&mut self, event_name: &str, act_set: Rc<RefCell<ActionSet>>) {
        self.event_map.add_order(event_name, act_set)
    }

    pub fn call(&mut self, event_name: &str, context: &PyDict) -> Result<()> {
        self.event_map.call_order(event_name, context)
    }
}

pub struct OrderMap {
    map: HashMap<String, Rc<RefCell<ActionSet>>>
}

impl OrderMap {
    pub fn new() -> Self {
        Self{map: HashMap::new()}
    }

    pub fn add_order(&mut self, order_name: &str, act_set: Rc<RefCell<ActionSet>>) {
        let action_entry = self.map.entry(order_name.to_string()).or_insert(ActionSet::create());
        *action_entry = act_set;
    }

    pub fn call_order(&mut self, act_name: &str, context: &PyDict) -> Result<()> {
        if let Some(action_set) = self.map.get_mut(act_name) {
            let gil = Python::acquire_gil();
            let python = gil.python();

            action_set.borrow_mut().call_all(python, context)?;
        }

        Ok(())
    }
}