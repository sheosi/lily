use serde::Serialize;
use snips_nlu_lib::SnipsNluEngine;
use std::collections::HashMap;
use std::path::Path;
use std::io::{Read, Write};

#[derive(Serialize)]
struct NluTrainSet {
    entities: HashMap<String, HashMap<String,()>>,
    intents: HashMap<String, Intent>
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
    entity: Option<String>,
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

    pub fn train(&self, path: &Path) {

    	// Prepare data
    	fn make_utt(input: &String) -> Utterance {
    		Utterance{data: vec![UtteranceData{text: input.to_string(), entity: None, slot_name: None}]}
    	}

    	let mut intents: HashMap<String, Intent> = HashMap::new();
    	for (name, utts) in self.intents.iter() {
    		let utterances = utts.into_iter().map(make_utt).collect();
    		intents.insert(name.to_string(), Intent{utterances});
    	}

    	let train_set = NluTrainSet{entities: HashMap::new(), intents};

    	// Output JSON
    	let train_set = serde_json::to_string(&train_set).unwrap();

    	// Write to file
    	let train_set_path = path.join("nlu_train_set.json");
    	let mut train_file = std::fs::OpenOptions::new().read(true).write(true).create(true).open(train_set_path).unwrap();
    	let mut old_train_file: String = String::new();
    	train_file.read_to_string(&mut old_train_file).unwrap();

    	// Make sure it's different, otherwise no need to train it
    	if old_train_file != train_set {
	    	train_file.write_all(train_set[..].as_bytes()).unwrap();
	    	train_file.sync_all().unwrap();

			std::process::Command::new("snips-nlu").arg("train").arg(train_set).arg(path).spawn().expect("Failed to open snips-nlu binary").wait().expect("snips-nlu failed it's execution, maybe some argument it's wrong?");
		}
    }
}

pub struct Nlu {
    engine: SnipsNluEngine,
}

impl Nlu {
    pub fn new() -> Nlu {
        let engine = SnipsNluEngine::from_path("engine").unwrap();

        Nlu { engine }
    }

    pub fn parse(&self, input: &str) -> snips_nlu_lib::Result<snips_nlu_ontology::IntentParserResult> {
        self.engine.parse_with_alternatives(&input, None, None, 3, 3)
    }


    pub fn to_json(res: &snips_nlu_ontology::IntentParserResult ) -> String {
        serde_json::to_string_pretty(&res).unwrap()
    }
}