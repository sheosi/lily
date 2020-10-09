//! Implement the interfaces that the 'order' signal and 'say' action will use

mod directvoice;

#[cfg(feature = "mqtt_interface")]
mod mqtt;

pub use self::directvoice::*;

#[cfg(feature = "mqtt_interface")]
pub use self::mqtt::*;

use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use crate::config::Config;
use crate::signals::SignalEventShared;
use crate::stt::DecodeRes;

use anyhow::Result;
use pyo3::{types::PyDict, Py};

pub type SharedOutput = Arc<Mutex<dyn UserInterfaceOutput>>;

thread_local! {
    pub static CURR_INTERFACE: RefCell<SharedOutput> = RefCell::new(UserInterfaceFactory::default_output());
}

pub trait UserInterface {
    fn interface_loop<F: FnMut( Option<DecodeRes>, SignalEventShared)->Result<()>> (&mut self, config: &Config, signal_event: SignalEventShared, base_context: &Py<PyDict>, callback: F) -> Result<()>;
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

