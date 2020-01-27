use std::collections::HashMap;
use std::path::Path;
use std::io::{Read, Write, Seek, SeekFrom};

use unic_langid::LanguageIdentifier;
use serde::Serialize;
use snips_nlu_lib::SnipsNluEngine;
use anyhow::{anyhow, Result};


#[derive(Serialize)]
struct NluTrainSet {
    entities: HashMap<String, HashMap<String,()>>,
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

pub struct NluManager {
    intents: Vec<(String, Vec<String>)>

}

impl NluManager {
    pub fn new() -> Self {
        NluManager {intents: vec![]}
    }

    pub fn add_intent(&mut self, order_name: &str, phrases: Vec<String>) {
        self.intents.push((order_name.to_string(), phrases));
    }

    pub fn train(&self, train_set_path: &Path, engine_path: &Path, lang: &LanguageIdentifier) -> Result<()> {

    	// Prepare data
    	fn make_utt(input: &String) -> Utterance {
    		Utterance{data: vec![UtteranceData{text: input.to_string(), entity: None, slot_name: None}]}
    	}

    	let mut intents: HashMap<String, Intent> = HashMap::new();
    	for (name, utts) in self.intents.iter() {
    		let utterances = utts.into_iter().map(make_utt).collect();
    		intents.insert(name.to_string(), Intent{utterances});
    	}

    	let train_set = NluTrainSet{entities: HashMap::new(), intents, language: lang.get_language().to_string()};

    	// Output JSON
    	let train_set = serde_json::to_string(&train_set)?;

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

            //Clean engine folder
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

pub struct Nlu {
    engine: SnipsNluEngine,
}

impl Nlu {
    pub fn new(engine_path: &Path) -> Result<Nlu> {
        let engine = SnipsNluEngine::from_path(engine_path).map_err(|err|anyhow!("Error while creating NLU engine, details: {:?}", err))?; 

        Ok(Nlu { engine })
    }

    pub fn parse(&self, input: &str) -> snips_nlu_lib::Result<snips_nlu_ontology::IntentParserResult> {
        self.engine.parse_with_alternatives(&input, None, None, 3, 3)
    }


    pub fn to_json(res: &snips_nlu_ontology::IntentParserResult ) -> Result<String> {
        Ok(serde_json::to_string_pretty(&res)?)
    }
}