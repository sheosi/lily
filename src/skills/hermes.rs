// Standard library
use std::sync::{Arc, Mutex};

// This crate
use crate::actions::LocalActionRegistry;
use crate::exts::LockIt;
use crate::queries::LocalQueryRegistry;
use crate::signals::LocalSignalRegistry;
use crate::skills::Loader;

// Other crates
use anyhow::Result;
use rumqttc::{AsyncClient, QoS};
use unic_langid::LanguageIdentifier;
pub struct HermesLoader {

}

impl HermesLoader {
    pub fn new() -> Self {
        Self {}
    }
}

impl Loader for HermesLoader {
    fn load_skills(&mut self,
        _base_sigreg: &LocalSignalRegistry,
        _base_actreg: &LocalActionRegistry,
        _base_queryreg: &LocalQueryRegistry,
        _langs: &Vec<LanguageIdentifier>) -> Result<()> {


        Ok(())
    }
}

pub struct HermesApiIn {

}

impl HermesApiIn {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn subscribe(client: &Arc<Mutex<AsyncClient>>) -> Result<()> {
        let client_raw = client.lock_it();
        

        Ok(())
    }
}

pub struct HermesApiOut {
}

impl HermesApiOut {
    pub fn new() -> Self {
        Self {}
    }
}