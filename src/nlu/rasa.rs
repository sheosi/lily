// Note: Rasa needs tensorflow_text
use std::collections::HashMap;
use std::convert::{Into, TryInto};
use std::path::Path;
use std::process::{Child, Command};

use crate::nlu::{compare_sets_and_train, try_open_file_and_check, write_contents};
use crate::nlu::{EntityDef, EntityData, Nlu, NluManager, NluResponse, NluResponseSlot, NluUtterance};

use anyhow::Result;
use reqwest::blocking;
use maplit::hashmap;
use serde::{Serialize, Deserialize};
use serde_yaml::Value;
use thiserror::Error;
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
struct RasaResponse {
    pub intent: RasaIntent,
    pub entities: Vec<RasaNluEntity>, // This one lacks the extractor field, but whe don't need it
    pub intent_ranking: Vec<RasaIntent>,
}

#[derive(Deserialize, Debug)]
pub struct RasaIntent {
    pub name: String,
    pub confidence: f32
}

impl Nlu for RasaNlu {
    fn parse (&self, input: &str) -> Result<NluResponse> {
        let map = hashmap!{"text" => input};

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

#[derive(Deserialize, Serialize, Debug)]
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

    fn make_pipeline() -> Vec<HashMap<String, Value>> {

        vec![
            hashmap!{"name".to_owned() => "ConveRTTokenizer".into()},
            hashmap!{"name".to_owned() => "ConveRTFeaturizer".into()},
            hashmap!{"name".to_owned() => "RegexFeaturizer".into()},
            hashmap!{"name".to_owned() => "LexicalSyntacticFeaturizer".into()},
            hashmap!{"name".to_owned() => "CountVectorsFeaturizer".into()},
            hashmap!{"name".to_owned() => "CountVectorsFeaturizer".into(),
                    "analyzer".to_owned() => "char_wb".into(),
                    "min_ngram".to_owned() => 1.into(),
                    "max_ngram".to_owned() => 1.into()},
            hashmap!{"name".to_owned() => "DIETClassifier".into(),
                     "epochs".to_owned() => 100.into()},
            hashmap!{"name".to_owned() => "EntitySynonymMapper".into()},
            hashmap!{"name".to_owned() => "ResponseSelector".into(),
                     "epochs".to_owned() => 100.into()}

        ]
    }

    fn make_train_conf(lang: &LanguageIdentifier) -> Result<String> {
        let conf = RasaNluTrainConfig{
            language: lang.to_string(),
            pipeline: Self::make_pipeline(),
            data: None,
            policies: None
        };
        Ok(serde_yaml::to_string(&conf)?)
    }

    fn make_train_set_json(self) -> Result<String> {
        let common_examples: Vec<RasaNluCommmonExample> = transform_intents(self.intents);
        
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

    fn add_entity(&mut self, _name: &str, def: EntityDef) {
        self.synonyms.extend(def.data.into_iter());
    }

    fn train(self, train_set_path: &Path, engine_path: &Path, lang: &LanguageIdentifier) -> Result<()> {

        let train_set = self.make_train_set_json()?;
        let train_conf = Self::make_train_conf(lang)?;

        let engine_path = Path::new(engine_path);
        let rasa_path = train_set_path.parent().expect("Failed to get rasa's path from data's path");
        if let Some(mut file) = try_open_file_and_check(&rasa_path.join("conf.yml"), &train_conf)? {
            write_contents(&mut file, &train_conf)?;
        };


        // Make sure it's different, otherwise no need to train it
        compare_sets_and_train(train_set_path, &train_set, engine_path, 
            ||{
                std::process::Command::new("rasa").args(&["train", "nlu"])
                .spawn().expect("Failed to execute rasa")
                .wait().expect("rasa failed it's training, maybe some argument it's wrong?");
            })?;

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
            slots: self.entities.into_iter()
                  .map(|e|NluResponseSlot{value: e.value, name: e.entity})
                  .collect()
        }
    }
}

#[derive(Error, Debug)]
pub enum RasaError {
    #[error("Failed to write training configuration")]
    CantWriteConf(#[from]std::io::Error)
}