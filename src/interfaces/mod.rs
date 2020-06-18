//! Implement the interfaces that the 'order' signal and 'say' action will use

mod directvoice;
pub use self::directvoice::*;

use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use anyhow::Result;

type SharedOutput = Arc<Mutex<dyn UserInterfaceOutput>>;

thread_local! {
    pub static CURR_INTERFACE: RefCell<SharedOutput> = RefCell::new(UserInterfaceFactory::default_output());
}

pub trait UserInterface {
    fn get_output(&self) -> SharedOutput;
}

pub trait UserInterfaceOutput {
    fn answer(&mut self, input: &str) -> Result<()>;
}

pub struct DummyInterface{}

impl DummyInterface {
    pub fn new() -> Self {
        Self{}
    }
}

impl UserInterfaceOutput for DummyInterface {
    fn answer(&mut self, _input: &str) -> Result<()> {
        Ok(())
    }
}

struct UserInterfaceFactory {}

impl UserInterfaceFactory {
    pub fn default_output() -> SharedOutput {
        Arc::new(Mutex::new(DummyInterface::new()))
    }
}
