use std::collections::HashMap;
use std::convert::{Into, TryInto};
use std::io::{Read, Write, Seek, SeekFrom};
use std::path::Path;
use std::process::{Child, Command};

use crate::nlu::{EntityDef, EntityData, Nlu, NluManager, NluResponse, NluResponseSlot, NluUtterance};

use anyhow::Result;
use reqwest::blocking;
use serde::{Serialize, Deserialize};
use serde_yaml::Value;
use unic_langid::LanguageIdentifier;

pub struct RasaNlu {
    client: blocking::Client,
    process: Child
}

impl RasaNlu {
    pub fn new(model_path: &Path) -> Self {
        let process = Command::new("rasa").args(&["run", "--enable-api", "-m", model_path.as_os_str().to_str().unwrap()]).spawn().unwrap();
        let client = blocking::Client::new();

        Self{client, process}

    }
}

#[derive(Deserialize, Debug)]
pub struct RasaResponse {
    pub intent: RasaIntent,
    pub entities: Vec<RasaEntity>,
    pub intent_ranking: Vec<RasaIntent>,
}

#[derive(Deserialize, Debug)]
pub struct RasaIntent {
    pub name: String,
    pub confidence: f32
}

#[derive(Deserialize, Debug)]
pub struct RasaEntity {

}

impl Nlu for RasaNlu {
    fn parse (&self, input: &str) -> Result<NluResponse> {
        let mut map = HashMap::new();
        map.insert("text", input);

        let resp: RasaResponse = self.client.post("localhost:5005/model/parse")
                                       .json(&map).send()?
                                       .json()?;

        Ok(resp.into())
        
    }
}

#[derive(Serialize)]
struct RasaTrainSet {
    #[serde(rename = "rasa_nlu_data")]
    data: RasaNluData
}

#[derive(Serialize)]
struct RasaNluData {
    common_examples: Vec<RasaNluCommmonExample>,
    regex_features: Vec<RasaNluRegexFeature>,
    lookup_tables: Vec<RasaNluLookupTable>,
    entity_synonyms: Vec<EntityData>
}

#[derive(Serialize)]
struct RasaNluCommmonExample {
    text: String,
    intent: String,
    entities: Vec<RasaNluEntity>
}

#[derive(Serialize)]
struct RasaNluRegexFeature {
    name: String,
    pattern: String
}

#[derive(Serialize)]
struct RasaNluLookupTable {
        //NYI
}

#[derive(Serialize)]
struct RasaNluEntity {
    start: u32,
    end: u32,
    value: String,
    entity: String
}

#[derive(Serialize)]
struct RasaNluTrainConfig {
    language: String,
    pipeline: Vec<HashMap<String, Value>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    policies: Option<Vec<HashMap<String, Value>>>,
}

#[derive(Serialize)]
struct RasaNluPipelineElement {
    name: String,
}


pub struct RasaNluManager {
    intents: Vec<(String, Vec<NluUtterance>)>,
    synonyms: Vec<EntityData>
}

impl RasaNluManager {
    pub fn new() -> Self {
        Self{intents: vec![], synonyms: vec![]}
    }

    fn make_train_set_json(self, lang: &LanguageIdentifier) -> Result<String> {
        let mut common_examples: Vec<RasaNluCommmonExample> = transform_intents(self.intents);

        let data = RasaNluData{
            common_examples,
            entity_synonyms: self.synonyms,
            regex_features: vec![],
            lookup_tables: vec![]
        };
        let train_set = RasaTrainSet{data};

        // Output JSON
        Ok(serde_json::to_string(&train_set)?)
    }
}

impl NluManager for RasaNluManager {
    fn add_intent(&mut self, order_name: &str, phrases: Vec<NluUtterance>) {
        self.intents.push((order_name.to_string(), phrases));
    }

    fn add_entity(&mut self, name: &str, def: EntityDef) {
        self.synonyms.extend(def.data.into_iter());
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
            std::process::Command::new("rasa").args(&["train", "nlu"]).spawn().expect("Failed to execute rasa").wait().expect("rasa failed it's training, maybe some argument it's wrong?");

        }

        Ok(())
    }
}

fn transform_intents(org: Vec<(String, Vec<NluUtterance>)>) -> Vec<RasaNluCommmonExample> {
    let mut result: Vec<RasaNluCommmonExample> = Vec::with_capacity(org.len());
    for (name, utts) in org.into_iter() {
        for utt in utts.into_iter() {
            let ex = 
                match utt {
                    NluUtterance::Direct(text) => RasaNluCommmonExample {
                        text,
                        intent: name.clone(),
                        entities: vec![]
                    },
                    NluUtterance::WithEntities{text, entities: conf_entities} => {
                        let mut entities = Vec::with_capacity(conf_entities.len());
                        for (name_ent, entity) in conf_entities.into_iter() {
                            let start = text.find(&entity.example).unwrap();
                            let en = RasaNluEntity {
                                start: start.try_into().unwrap(),
                                end: (start + entity.example.len()).try_into().unwrap(),
                                value: entity.example,
                                entity: name_ent
                            };

                            entities.push(en);
                        }

                        RasaNluCommmonExample {
                            text,
                            intent: name.clone(),
                            entities
                        }
                    }
                };
            

            result.push(ex);
        }
    }

    result
}

impl Into<NluResponse> for RasaResponse {
    fn into(self) -> NluResponse {
        NluResponse {
            name: Some(self.intent.name),
            confidence: self.intent.confidence,
            //TODO: entities are not transferred in Rasa
            slots: self.entities.into_iter()
                  .map(|_e|NluResponseSlot{value: "".to_owned(), name: "".to_owned()})
                  .collect()
        }
    }
}