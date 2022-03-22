// Standard library

// This crate
use crate::skills::SkillLoader;

// Other crates
use anyhow::Result;
use async_trait::async_trait;
use unic_langid::LanguageIdentifier;

pub struct VapLoader {

}

impl VapLoader {
    pub fn new() -> Self {
        VapLoader {

        }
    }
}

#[async_trait(?Send)]
impl SkillLoader for VapLoader {
    fn load_skills(&mut self, _langs: &Vec<LanguageIdentifier>) -> Result<()> {
        Ok(())
    }

    
    async fn run_loader(&mut self) -> Result<()> {
        Ok(())
    }
}