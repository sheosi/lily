use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use crate::nlu::{Nlu, NluManager, NluUtterance};
use crate::vars::NLU_RASA_PATH;
use reqwest::blocking;
use serde::{Serialize, Deserialize};

pub struct RasaNlu {
    client: blocking::Client,
    process: Child
}

impl RasaNlu {
    pub fn new(model_path: &Path) -> Self {
        let mut map = HashMap::new();
        map.insert("text", "hello");
        let model_path = NLU_RASA_PATH.resolve().join();
        let process = Command::new("rasa").args(["run", "--enable-api", "-m", model_path).spawn().unwrap();

        let client = blocking::Client::new();

        Self{client, process}
    }
}

#[derive(Deserialize)]
struct RasaResponse {
    
}

impl Nlu for SnipsNlu {
    //type NluResult = 

    fn parse (&self, input: &str) -> Result<Self::NluResult, ()> {
        let resp: RasaResponse = self.client.get("localhost:5005/model/parse")
                                       .json(&map).send().unwrap()
                                       .json().unwrap();

        
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
    entity_synonyms: Vec<RasaNluEntitySynonym>
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
struct RasaNluEntitySynonym {
    value: String,
    synonyms: Vec<String>
}

#[derive(Serialize)]
struct RasaNluEntity {
    start: u32,
    end: u32,
    value: String,
    entity: String
}

pub struct RasaNluManager {
    intents: Vec<(String, <Vec<NluUtterance>>)>,
    entities: HashMap<String, En>
}

impl RasaNluManager {
    pub fn new() -> Self {
        Self{intents: vec![]}
    }
}

impl NluManager for RasaNluManager {
    fn add_intent(&mut self, order_name: &str, phrases: Vec<NluUtterance>) {
        self.intents.push((order_name.to_string(), phrases));
    }

    fn add_entity(&mut self, name: &str, def: EntityDef) {
        self.entities.insert(name.to_string(), def);
    }

    fn train(self, train_set_path: &Path, engine_path: &Path, lang: &LanguageIdentifier) -> Result<()> {
        //NYI   
    }
}