use std::collections::HashMap;
use std::convert::Into;
use std::fmt::{self, Debug};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::nlu::compare_sets_and_train;
use crate::nlu::{
    EntityDef, Nlu, NluManager, NluManagerStatic, NluResponse, NluResponseSlot, NluUtterance,
};
use crate::vars::{NLU_ENGINE_PATH, NLU_TRAIN_SET_PATH};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use regex::Regex;
use serde::Serialize;
use snips_nlu_lib::SnipsNluEngine;
use unic_langid::{langid, LanguageIdentifier};

use super::EntityData;

//// NluManager ///////////////////////////////////////////////////////////////////////////////////
#[derive(Serialize)]
struct NluTrainSet {
    entities: HashMap<String, SnipsEntityDef>,
    intents: HashMap<String, Intent>,
    language: String,
}

#[derive(Serialize)]
struct Intent {
    utterances: Vec<Utterance>,
}

#[derive(Serialize)]
struct Utterance {
    data: Vec<UtteranceData>,
}

#[derive(Serialize)]
struct UtteranceData {
    text: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    entity: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    slot_name: Option<String>,
}

#[derive(Serialize)]
pub struct EntityValue {
    value: String,
    synonnyms: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct SnipsEntityDef {
    data: Vec<EntityData>,
    automatically_extensible: bool,
    use_synonyms: bool,
}

impl From<EntityDef> for SnipsEntityDef {
    fn from(other: EntityDef) -> Self {
        Self {
            data: other.data,
            automatically_extensible: other.automatically_extensible,
            use_synonyms: true,
        }
    }
}

#[derive(Debug)]
pub struct SnipsNluManager {
    intents: Vec<(String, Vec<NluUtterance>)>,
    entities: HashMap<String, SnipsEntityDef>,
}

#[derive(Debug)]
enum SplitCapKind {
    Text,
    Entity,
}

impl Into<Utterance> for NluUtterance {
    fn into(self) -> Utterance {
        // Prepare data
        match self {
            NluUtterance::Direct(text) => Utterance {
                data: vec![UtteranceData {
                    text: text.to_string(),
                    entity: None,
                    slot_name: None,
                }],
            },
            NluUtterance::WithEntities { text, entities } => {
                // Capture "{something}" but ignore "\{something}", "something}" will also be ignored
                let re = Regex::new(r"[^\\]\(\s*\$([^}]+)\s*\)").expect("Error on regex");

                let construct_utt = |(text, kind): &(&str, SplitCapKind)| match kind {
                    SplitCapKind::Text => UtteranceData {
                        text: text.to_string(),
                        entity: None,
                        slot_name: None,
                    },
                    SplitCapKind::Entity => {
                        let ent_data = &entities[&text.to_string()];
                        UtteranceData {
                            text: ent_data.example.clone(),
                            entity: Some(ent_data.kind.clone()),
                            slot_name: Some(text.to_string()),
                        }
                    }
                };

                Utterance {
                    data: split_captures(&re, &text)
                        .iter()
                        .map(construct_utt)
                        .collect(),
                }
            }
        }
    }
}

fn split_captures<'a>(re: &'a Regex, input: &'a str) -> Vec<(&'a str, SplitCapKind)> {
    let mut cap_loc = re.capture_locations();
    let mut last_pos = 0;
    let mut result = Vec::new();

    while {
        re.captures_read_at(&mut cap_loc, input, last_pos);
        cap_loc.get(1).is_some()
    } {
        let (whole_s, whole_e) = cap_loc.get(0).expect("What? Couldn't get whole capture?");
        let (name_s, name_e) = cap_loc
            .get(1)
            .expect("Please make sure that the regex has a mandatory capture group");

        if whole_s != last_pos {
            // We need a character before '{' to check that is not '\{' since look-behind
            // is not implemented by regex
            result.push((&input[last_pos..whole_s + 1], SplitCapKind::Text));
        }

        result.push((&input[name_s..name_e], SplitCapKind::Entity));

        last_pos = whole_e;
    }

    // If nothing is found then put the whole thing as text
    if last_pos == 0 {
        result.push((input, SplitCapKind::Text));
    } else if last_pos != input.len() {
        result.push((&input[last_pos..input.len()], SplitCapKind::Text));
    }

    result
}

// Check if the Python module for Snips exists
fn python_has_module_path(module_path: &Path) -> Result<bool> {
    fn get_python_path() -> Result<Vec<String>> {
        let out = String::from_utf8(
            Command::new("python3")
                .args(&["-c", "import sys;print(sys.path)"])
                .output()?
                .stdout,
        )?;
        let reg = Regex::new("'([^'])'").expect("Regex failed");
        Ok(reg
            .find_iter(&out)
            .map(|m| m.as_str().to_string())
            .collect())
    }

    let sys_path = get_python_path()?;
    let mut found = false;
    for path_str in sys_path.iter() {
        let path = Path::new(path_str);
        let lang_path = path.join(module_path);
        if lang_path.exists() {
            found = true;
            break;
        }
    }

    Ok(found)
}

impl SnipsNluManager {
    fn make_train_set_json(&self, lang: &LanguageIdentifier) -> Result<String> {
        let mut intents: HashMap<String, Intent> = HashMap::new();
        for (name, utts) in self.intents.iter() {
            let utterances: Vec<Utterance> =
                utts.into_iter().map(|utt| utt.clone().into()).collect();
            intents.insert(name.to_string(), Intent { utterances });
        }

        let train_set = NluTrainSet {
            entities: self.entities.clone(),
            intents,
            language: lang.language.to_string(),
        };

        // Output JSON
        Ok(serde_json::to_string(&train_set)?)
    }

    fn is_lang_installed(lang: &LanguageIdentifier) -> Result<bool> {
        python_has_module_path(
            &Path::new("snips_nlu")
                .join("data")
                .join(lang.language.as_str()),
        )
    }
}

impl NluManager for SnipsNluManager {
    type NluType = SnipsNlu;
    fn ready_lang(&mut self, lang: &LanguageIdentifier) -> Result<()> {
        if !Self::is_lang_installed(lang)? {
            let lang_str = lang.language.as_str();
            let success = std::process::Command::new("snips-nlu")
                .args(&["download", lang_str])
                .status()
                .expect("Failed to open snips-nlu binary")
                .success();

            if success {
                Ok(())
            } else {
                Err(anyhow!(
                    "Failed to download NLU's data for language \"{}\"",
                    lang_str
                ))
            }
        } else {
            Ok(())
        }
    }

    fn add_intent(&mut self, order_name: &str, phrases: Vec<NluUtterance>) {
        self.intents.push((order_name.to_string(), phrases));
    }

    fn add_entity(&mut self, name: String, def: EntityDef) {
        self.entities.insert(name, def.into());
    }

    fn add_entity_value(&mut self, name: &str, value: String) -> Result<()> {
        let def = self
            .entities
            .get_mut(name)
            .ok_or_else(|| anyhow!("Entity {} does not exist", name))?;
        def.data.push(EntityData {
            value,
            synonyms: vec![],
        });
        Ok(())
    }

    fn train(
        &self,
        train_set_path: &Path,
        engine_path: &Path,
        lang: &LanguageIdentifier,
    ) -> Result<SnipsNlu> {
        let train_set = self.make_train_set_json(lang)?;

        // Write to file
        let engine_path = Path::new(engine_path);

        compare_sets_and_train(train_set_path, &train_set, engine_path, || {
            std::process::Command::new("snips-nlu")
                .arg("train")
                .args(&[train_set_path, engine_path])
                .spawn()
                .expect("Failed to open snips-nlu binary")
                .wait()
                .expect("snips-nlu failed it's execution, maybe some argument it's wrong?");
        })?;

        SnipsNlu::new(engine_path)
    }
}

impl NluManagerStatic for SnipsNluManager {
    fn new() -> Self {
        SnipsNluManager {
            intents: vec![],
            entities: HashMap::new(),
        }
    }

    fn list_compatible_langs() -> Vec<LanguageIdentifier> {
        vec![
            langid!("de"),
            langid!("en"),
            langid!("es"),
            langid!("fr"),
            langid!("it"),
            langid!("ja"),
            langid!("ko"),
            langid!("pt_br"),
            langid!("pt_pt"),
        ]
    }

    fn name() -> &'static str {
        "Snips"
    }

    fn get_paths() -> (PathBuf, PathBuf) {
        let train_path = NLU_TRAIN_SET_PATH.resolve().to_path_buf();
        let model_path = NLU_ENGINE_PATH.resolve().to_path_buf();

        (train_path, model_path)
    }
}

/// Nlu ////////////////////////////////////////////////////////////////////////////////////////////

pub struct SnipsNlu {
    engine: SnipsNluEngine,
}

impl Debug for SnipsNlu {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("SnipsNlu").finish()
    }
}

impl SnipsNlu {
    fn new(engine_path: &Path) -> Result<SnipsNlu> {
        let engine = SnipsNluEngine::from_path(engine_path)
            .map_err(|err| anyhow!("Error while creating NLU engine, details: {:?}", err))?;

        Ok(SnipsNlu { engine })
    }
}

#[async_trait(?Send)]
impl Nlu for SnipsNlu {
    async fn parse(&self, input: &str) -> Result<NluResponse> {
        self.engine
            .parse_with_alternatives(&input, None, None, 3, 3)
            .map(|r| {
                let a: NluResponse = r.into();
                a
            })
            .map_err(|_| anyhow!("Failed snips NLU"))
    }
}

impl From<snips_nlu_ontology::IntentParserResult> for NluResponse {
    fn from(res: snips_nlu_ontology::IntentParserResult) -> NluResponse {
        NluResponse {
            name: res.intent.intent_name,
            confidence: res.intent.confidence_score,
            slots: res
                .slots
                .into_iter()
                .map(|slt| NluResponseSlot {
                    value: slt.raw_value,
                    name: slt.slot_name,
                })
                .collect(),
        }
    }
}
