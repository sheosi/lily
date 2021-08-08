// Standard library


// This crate
use crate::actions::LocalActionRegistry;
use crate::queries::LocalQueryRegistry;
use crate::signals::LocalSignalRegistry;
use crate::skills::Loader;

// Other crates
use anyhow::Result;
use unic_langid::LanguageIdentifier;
pub struct RemoteLoader {

}

impl RemoteLoader {
    pub fn new() -> Self {
        Self {}
    }
}

impl Loader for RemoteLoader {
    fn load_skills(&mut self,
        _base_sigreg: &LocalSignalRegistry,
        _base_actreg: &LocalActionRegistry,
        _base_queryreg: &LocalQueryRegistry,
        _langs: &Vec<LanguageIdentifier>) -> Result<()> {


        Ok(())
    }
}