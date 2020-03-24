use std::collections::HashMap;
use std::path::Path;
use std::io::{Read, Write, Seek, SeekFrom};

use crate::python::try_translate;

use unic_langid::LanguageIdentifier;
use serde::Serialize;
use snips_nlu_lib::SnipsNluEngine;
use anyhow::{anyhow, Result};
use regex::Regex;

//// NluManager ///////////////////////////////////////////////////////////////////////////////////
#[derive(Serialize)]
struct NluTrainSet {
    entities: HashMap<String, EntityDef>,
    intents: HashMap<String, Intent>,
    language: String
}

#[derive(Serialize)]
struct Intent {
    utterances: Vec<Utterance>
}

#[derive(Serialize)]
struct Utterance {
    data: Vec<UtteranceData>
}

#[derive(Serialize)]
struct UtteranceData {
    text: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    entity: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    slot_name: Option<String>
}

#[derive(Serialize)]
pub struct EntityData {
    pub value: String,
    pub synonyms: Vec<String>
}

#[derive(Serialize)]
pub struct EntityDef {
    pub data: Vec<EntityData>,
    pub use_synonyms: bool,
    pub automatically_extensible: bool
}

#[derive(Serialize)]
pub struct EntityValue {
    value: String,
    synonnyms: String
}

pub trait NluManager {
    fn add_intent(&mut self, order_name: &str, phrases: Vec<NluUtterance>);
    fn add_entity(&mut self, name:&str, def: EntityDef);

    // Consume the struct so that we can reuse memory
    fn train(self, train_set_path: &Path, engine_path: &Path, lang: &LanguageIdentifier) -> Result<()>; 
}

pub struct SnipsNluManager {
    intents: Vec<(String, Vec<NluUtterance>)>,
    entities: HashMap<String, EntityDef>
}

impl SnipsNluManager {
    fn new() -> Self {
        SnipsNluManager {intents: vec![], entities:HashMap::new()}
    }
}

#[derive(Clone)]
pub struct EntityInstance {
    pub kind: String,
    pub example: String
}

pub enum NluUtterance{
    Direct(String),
    WithEntities {text: String, entities: HashMap<String, EntityInstance>}
}

#[derive(Debug)]
enum SplitCapKind {
    Text, Entity
}

impl Into<Utterance> for NluUtterance {
    fn into(self) -> Utterance {
        // Capture "{something}" but ignore "\{something}", "something}" will also be ignored
        let re = Regex::new(r"[^\\]\{\s*\$([^}]+)\s*\}").unwrap();

        // Prepare data
        match self {
            NluUtterance::Direct(text) => {
                Utterance{data: vec![UtteranceData{text: text.to_string(), entity: None, slot_name: None}]}
            },
            NluUtterance::WithEntities {text, entities} => {                    

                let construct_utt = |(text, kind):&(&str, SplitCapKind)| {
                    match kind {
                        SplitCapKind::Text => UtteranceData{text: text.to_string(), entity: None, slot_name: None},
                        SplitCapKind::Entity => {
                            println!("Entity: {:?}", text);
                            let ent_data = &entities[&text.to_string()];
                            UtteranceData{text: try_translate(&ent_data.example).unwrap(), entity: Some(ent_data.kind.clone()), slot_name: Some(text.to_string())}
                        }
                    }
                };
                
                Utterance{data: split_captures(&re, &text).iter().map(construct_utt).collect()}
            }
        }
            
        
    }
}


fn split_captures<'a>(re: &'a Regex, input: &'a str) ->  Vec<(&'a str, SplitCapKind)>{
    let mut cap_loc = re.capture_locations();
    let mut last_pos = 0;
    let mut result = Vec::new();

    while {re.captures_read_at(&mut cap_loc, input, last_pos); cap_loc.get(1).is_some()} {
        let (whole_s, whole_e) = cap_loc.get(0).unwrap();
        let (name_s, name_e) = cap_loc.get(1).unwrap();

        if whole_s != last_pos {
            // We need a character before '{' to check that is not '\{' since look-behind
            // is not implemented by regex
            result.push((&input[last_pos..whole_s + 1],SplitCapKind::Text));
        }

        result.push((&input[name_s..name_e], SplitCapKind::Entity));

        last_pos = whole_e;
    }

    // If nothing is found then put the whole thing as text
    if last_pos == 0 {
        result.push((input, SplitCapKind::Text));
    }
    else if last_pos != input.len() {
        result.push((&input[last_pos..input.len()], SplitCapKind::Text));
    }

    println!("{:?}", result);
    result
}

impl SnipsNluManager {
    fn make_train_set_json(self, lang: &LanguageIdentifier) -> Result<String> {
        let mut intents: HashMap<String, Intent> = HashMap::new();
        for (name, utts) in self.intents.into_iter() {
            let utterances: Vec<Utterance> = utts.into_iter().map(|utt| utt.into()).collect();
            intents.insert(name.to_string(), Intent{utterances});
        }

        let train_set = NluTrainSet{entities: self.entities, intents, language: lang.get_language().to_string()};

        // Output JSON
        Ok(serde_json::to_string(&train_set)?)

    }
}

impl NluManager for SnipsNluManager {
    fn add_intent(&mut self, order_name: &str, phrases: Vec<NluUtterance>) {
        self.intents.push((order_name.to_string(), phrases));
    }

    fn add_entity(&mut self, name: &str, def: EntityDef) {
        self.entities.insert(name.to_string(), def);
    }

    fn train(self, train_set_path: &Path, engine_path: &Path, lang: &LanguageIdentifier) -> Result<()> {

    	let train_set = self.make_train_set_json(lang)?;

    	// Write to file
    	let mut train_file = std::fs::OpenOptions::new().read(true).write(true).create(true).open(train_set_path);
        let should_write = {
            if let Ok(train_file) = &mut train_file {
                let mut old_train_file: String = String::new();
                train_file.read_to_string(&mut old_train_file)?;
                old_train_file != train_set
            }
            else {
                if let Some(path_parent) = train_set_path.parent() {
                    std::fs::create_dir_all(path_parent)?;
                }
                train_file = std::fs::OpenOptions::new().read(true).write(true).create(true).open(train_set_path);

                false
            }
        };
    	
        let engine_path = Path::new(engine_path);

    	// Make sure it's different, otherwise no need to train it
    	if should_write {
            // Create parents
            if let Some(path_parent) = engine_path.parent() {
                std::fs::create_dir_all(path_parent)?;
            }

            // Clean engine folder
            if engine_path.is_dir() {
                std::fs::remove_dir_all(engine_path)?;
            }

            // Write train file
            let mut train_file = train_file?;
            train_file.set_len(0)?; // Truncate file
            train_file.seek(SeekFrom::Start(0))?; // Start from the start
	    	train_file.write_all(train_set[..].as_bytes())?;
	    	train_file.sync_all()?;

            // Train engine
			std::process::Command::new("snips-nlu").arg("train").arg(train_set_path).arg(engine_path).spawn().expect("Failed to open snips-nlu binary").wait().expect("snips-nlu failed it's execution, maybe some argument it's wrong?");
		}

        Ok(())
    }
}

/// Nlu ////////////////////////////////////////////////////////////////////////////////////////////

pub struct SnipsNlu {
    engine: SnipsNluEngine,
}

impl SnipsNlu {
    fn new(engine_path: &Path) -> Result<SnipsNlu> {
        let engine = SnipsNluEngine::from_path(engine_path).map_err(|err|anyhow!("Error while creating NLU engine, details: {:?}", err))?; 

        Ok(SnipsNlu { engine })
    }
}

impl Nlu for SnipsNlu {
    type NluResult = snips_nlu_ontology::IntentParserResult;

    fn parse(&self, input: &str) -> snips_nlu_lib::Result<snips_nlu_ontology::IntentParserResult> {
        self.engine.parse_with_alternatives(&input, None, None, 3, 3)
    }


    /*fn to_json(res: &snips_nlu_ontology::IntentParserResult ) -> Result<String> {
        Ok(serde_json::to_string_pretty(&res)?)
    }*/
}

pub trait Nlu {
    type NluResult;

    fn parse(&self, input: &str) -> snips_nlu_lib::Result<Self::NluResult>;
}

pub struct NluFactory {}

impl NluFactory {
    pub fn new_nlu(engine_path: &Path) -> Result<SnipsNlu> {
        SnipsNlu::new(engine_path)
    }

    pub fn new_manager() -> SnipsNluManager {
        SnipsNluManager::new()
    }
}

