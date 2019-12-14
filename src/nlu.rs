use unic_langid::LanguageIdentifier;
use serde::Serialize;
use snips_nlu_lib::SnipsNluEngine;
use std::collections::HashMap;
use std::path::Path;
use std::io::{Read, Write, Seek, SeekFrom, ErrorKind};


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

    pub fn train(&self, train_set_path: &Path, engine_path: &Path, lang: &LanguageIdentifier) {

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
    	let train_set = serde_json::to_string(&train_set).unwrap();

    	// Write to file
    	let mut train_file = std::fs::OpenOptions::new().read(true).write(true).create(true).open(train_set_path).unwrap();
    	let mut old_train_file: String = String::new();
    	train_file.read_to_string(&mut old_train_file).unwrap();
        let engine_path = Path::new(engine_path);

    	// Make sure it's different, otherwise no need to train it
    	if old_train_file != train_set || !engine_path.is_dir() {
            // Create parents
            std::fs::create_dir_all(engine_path.parent().unwrap()).unwrap();
            std::fs::create_dir_all(train_set_path.parent().unwrap()).unwrap();

            //Clean engine folder
            if engine_path.is_dir() {
                std::fs::remove_dir_all(engine_path).unwrap();
            }

            // Write train file
            train_file.set_len(0).unwrap(); // Truncate file
            train_file.seek(SeekFrom::Start(0)).unwrap(); // Start from the start
	    	train_file.write_all(train_set[..].as_bytes()).unwrap();
	    	train_file.sync_all().unwrap();

            // Train engine
			std::process::Command::new("snips-nlu").arg("train").arg(train_set_path).arg(engine_path).spawn().expect("Failed to open snips-nlu binary").wait().expect("snips-nlu failed it's execution, maybe some argument it's wrong?");
		}
    }
}

pub struct Nlu {
    engine: SnipsNluEngine,
}

impl Nlu {
    pub fn new(engine_path: &Path) -> Nlu {
        let engine = SnipsNluEngine::from_path(engine_path).unwrap();

        Nlu { engine }
    }

    pub fn parse(&self, input: &str) -> snips_nlu_lib::Result<snips_nlu_ontology::IntentParserResult> {
        self.engine.parse_with_alternatives(&input, None, None, 3, 3)
    }


    pub fn to_json(res: &snips_nlu_ontology::IntentParserResult ) -> String {
        serde_json::to_string_pretty(&res).unwrap()
    }
}