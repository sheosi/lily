pub mod server_interface;

// Standard library
use std::collections::{HashMap};
use std::fmt::{self, Debug};
use std::sync::{Arc, Mutex};

// This crate
use crate::actions::{ActionContext, ActionSet};
use crate::config::Config;
use crate::nlu::{EntityData, EntityDef, EntityInstance, Nlu, NluManager, NluManagerConf, NluManagerStatic, NluResponseSlot, NluUtterance};
use crate::python::{try_translate, try_translate_all};
use crate::stt::DecodeRes;
use crate::signals::{ActMap, Signal, SignalEventShared};
use crate::vars::{MIN_SCORE_FOR_ACTION, POISON_MSG};
use self::server_interface::MqttInterface;

// Other crates
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use log::{info, error, warn};
use serde::{Deserialize, Deserializer, de::{self, SeqAccess, Visitor}, Serialize};
use unic_langid::LanguageIdentifier;

#[cfg(not(feature = "devel_rasa_nlu"))]
use crate::nlu::SnipsNluManager;

#[cfg(feature = "devel_rasa_nlu")]
use crate::nlu::RasaNluManager;

#[derive(Debug, Clone, Deserialize)]
struct YamlEntityDef {
    data: Vec<String>
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum OrderKind {
    Ref(String),
    Def(YamlEntityDef)
}

#[derive(Clone, Deserialize)]
struct OrderEntity {
    kind: OrderKind,
    example: String
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OrderData {
    Direct(String),
    WithEntities{text: String, entities: HashMap<String, OrderEntity>}
}



#[derive(Clone, Debug, Serialize)]
pub struct StringList {
    data: Vec<String>
}

impl StringList {
    pub fn new() -> Self {
        Self{data: Vec::new()}
    }
    pub fn from_vec(vec: Vec<String>) -> Self {
        Self{ data: vec}
    }

    /// Returns an aggregated vector with the translations of all entries
    pub fn into_translation(self, lang: &LanguageIdentifier) -> Result<Vec<String>,Vec<String>> {
        let lang_str = lang.to_string();

        let (utts_data, failed):(Vec<_>,Vec<_>) = self.data.into_iter()
        .map(|utt|try_translate_all(&utt, &lang_str))
        .partition(Result::is_ok);

        if failed.is_empty() {
            let utts = utts_data.into_iter().map(Result::unwrap)
            .flatten().collect();

            Ok(utts)
        }
        else {
            let failed = failed.into_iter().map(Result::unwrap)
            .flatten().collect();
            Err(failed)            
        }
    }
}

struct StringListVisitor;

impl<'de> Visitor<'de> for StringListVisitor {
    type Value = StringList;
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("Either a string or a list containing strings")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E> where E: de::Error{
        Ok(StringList{data:vec![v.to_string()]})
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error> where A: SeqAccess<'de> {
        let mut res = StringList{data: Vec::with_capacity(seq.size_hint().unwrap_or(1))};
        loop {
            match seq.next_element()? {
                Some(val) => {res.data.push(val)},
                None => {break}
            }
        }

        return Ok(res);   
    }
}

impl<'de> Deserialize<'de> for StringList {
    fn deserialize<D>(deserializer: D) -> Result<StringList, D::Error>
    where D: Deserializer<'de> {
        deserializer.deserialize_i32(StringListVisitor)
    }
}



#[derive(Debug, Deserialize)]
struct SlotData {
    #[serde(rename="type")]
    slot_type: OrderKind,
    #[serde(default="false_val")]
    required: bool,
    prompt: String,
    reprompt: String
}
fn false_val() -> bool {false}
#[derive(Debug, Deserialize)]
pub struct IntentData {

    #[serde(alias = "samples", alias = "sample")]
    utts:  StringList,
    #[serde(default="empty_map")]
    slots: HashMap<String, SlotData>,
    #[serde(flatten)]
    pub hook: Hook
}

#[derive(Clone, Debug, Deserialize)]
pub enum Hook {
    #[serde(rename="query")]
    Query(String),
    #[serde(rename="action")]
    Action(String),
    #[serde(rename="signal")]
    Signal(String)
}

fn empty_map() -> HashMap<String, SlotData> {
    HashMap::new()
}

fn add_intent_to_nlu<N: NluManager + NluManagerStatic + NluManagerConf>(
    nlu_man: &mut HashMap<LanguageIdentifier, NluState<N>>,
    sig_arg: IntentData,
    intent_name: &str,
    skill_name: &str,
    langs: &Vec<LanguageIdentifier>
) -> Result<()> {
    
    for lang in langs {

        //First, register all slots
        let mut slots_res:HashMap<String, EntityInstance> = HashMap::new();
        for (slot_name, slot_data) in sig_arg.slots.iter() {

            // Handle that slot types might be defined on the spot
            let ent_kind_name = match slot_data.slot_type.clone() {
                OrderKind::Ref(name) => name,
                OrderKind::Def(def) => {
                    let name = format!("_{}__{}_", skill_name, slot_name);
                    nlu_man.get_mut(lang).expect("Language not registered").get_mut_nlu_man()
                    .add_entity(&name, def.try_into_with_trans(lang)?);
                    name
                }
            };

            let slot_example = "".to_string();

            slots_res.insert(
                slot_name.to_string(),
                EntityInstance {
                    kind: ent_kind_name,
                    example: {
                        match try_translate(&slot_example, &lang.to_string()) {
                            Ok(trans) =>  trans,
                            Err(err) => {
                                warn!("Failed to do translation of \"{}\", error: {:?}", &slot_example, err);
                                slot_example
                            }
                        }
                    },
                },
            );
        }

        // Now register all utterances
        match sig_arg.utts.clone().into_translation(lang) {
            Ok(t) => {
                let utts = t.into_iter().map(|utt|
                if slots_res.is_empty() {
                    NluUtterance::Direct(utt)
                }
                else {
                    NluUtterance::WithEntities {
                        text: utt,
                        entities: slots_res.clone(),
                    }
                }).collect();

                nlu_man.get_mut(lang).expect("Input language was not present before").get_mut_nlu_man()
                .add_intent(intent_name, utts);
            }
            Err(failed) => {
                if failed.len() == 1 {
                    error!("Sample '{}' of '{}'  couldn't be translated", failed[0], skill_name)
                }
                else {
                    error!("Samples '{}' of '{}' couldn't be translated", failed.join(", "), skill_name)
                }
            }

        }
        
            
    }
    Ok(())
}


#[derive(Debug)]
enum NluState<M: NluManager + NluManagerStatic + NluManagerConf> {
    Training(M), Done(M::NluType), InProcess
}

impl<M: NluManager + NluManagerStatic + NluManagerConf> NluState<M> {
    fn get_mut_nlu_man(&mut self) -> &mut M {
        match self {
            NluState::Training(m) => {m},
            NluState::Done(_)=> {panic!("Can't get Nlu Manager when the NLU it's already trained")},
            NluState::InProcess => {panic!("Can't call this while training the NLU")}
        }
    }
}

#[derive(Debug)]
pub struct SignalOrder<M: NluManager + NluManagerStatic + NluManagerConf + Debug> {
    intent_map: ActMap,
    nlu: HashMap<LanguageIdentifier, NluState<M>>,
    demangled_names: HashMap<String, String>,
    langs: Vec<LanguageIdentifier>
}

impl<M:NluManager + NluManagerStatic + NluManagerConf + Debug> SignalOrder<M> {
    pub fn new(langs: Vec<LanguageIdentifier>) -> Self {
        let mut managers = HashMap::new();
        for lang in &langs {
            managers.insert(lang.to_owned(), NluState::Training(M::new()));
        }
        SignalOrder {
            intent_map: ActMap::new(),
            nlu: managers,
            demangled_names: HashMap::new(),
            langs
        }
    }

    pub fn end_loading(&mut self) -> Result<()> {
        for lang in &self.langs {
            let (train_path, model_path) = M::get_paths();
            let err = || {anyhow!("Received language '{}' has not been registered", lang.to_string())};
            let val  = match self.nlu.insert(lang.clone(), NluState::InProcess).ok_or_else(err)? {
                NluState::Training(mut nlu_man) => {
                    if M::is_lang_compatible(lang) {
                        nlu_man.ready_lang(lang)?;
                        NluState::Done(nlu_man.train(&train_path, &model_path.join("main_model.json"), lang)?)
                    }
                    else {
                        Err(anyhow!("{} NLU is not compatible with the selected language", M::name()))?
                    }
                }
                _ => {
                    panic!("Called end_loading twice");
                }
            };
            self.nlu.insert(lang.to_owned(), val);

            info!("Initted Nlu");
        }

        Ok(())
    }

    pub async fn received_order(&mut self, decode_res: Option<DecodeRes>, event_signal: SignalEventShared, base_context: &ActionContext, lang: &LanguageIdentifier) -> Result<()> {
        match decode_res {
            None => event_signal.lock().expect(POISON_MSG).call("empty_reco", base_context),
            Some(decode_res) => {

                if !decode_res.hypothesis.is_empty() {
                    const ERR_MSG: &str = "Received language to the NLU was not registered";
                    match self.nlu.get_mut(&lang).expect(ERR_MSG) {
                        NluState::Done(ref mut nlu) => {
                            let result = nlu.parse(&decode_res.hypothesis).await.map_err(|err|anyhow!("Failed to parse: {:?}", err))?;
                            info!("{:?}", result);

                            // Do action if at least we are 80% confident on
                            // what we got
                            if result.confidence >= MIN_SCORE_FOR_ACTION {
                                if let Some(intent_name) = result.name {
                                    info!("Let's call an action");
                                    let mut slots_context = add_slots(base_context,result.slots);
                                    slots_context.set("intent".to_string(), self.demangle(&intent_name).to_string());
                                    self.intent_map.call_mapping(&intent_name, &slots_context);
                                    info!("Action called");
                                }
                                else {
                                    event_signal.lock().expect(POISON_MSG).call("unrecognized", &base_context);
                                }
                            }
                            else {
                                event_signal.lock().expect(POISON_MSG).call("unrecognized", &base_context);
                            }

                        },
                        _ => {
                            panic!("received_order can't be called before end_loading")
                        }
                    }
                }
                else {
                    event_signal.lock().expect(POISON_MSG).call("empty_reco", &base_context);
                }
            }
        }
    Ok(())
    }

    pub async fn record_loop(&mut self, signal_event: SignalEventShared, config: &Config, base_context: &ActionContext, curr_lang: &Vec<LanguageIdentifier>) -> Result<()> {
        let mut interface = MqttInterface::new(curr_lang)?;
        interface.interface_loop(config, signal_event, base_context, self).await
    }
}


fn add_slots(base_context: &ActionContext, slots: Vec<NluResponseSlot>) -> ActionContext {
    let mut result = base_context.clone();
    for slot in slots.into_iter() {
        result.set(slot.name, slot.value);
    }

    result
}

impl<M:NluManager + NluManagerStatic + NluManagerConf + Debug>  SignalOrder<M> {
    fn mangle(intent_name: &str, skill_name: &str) -> String {
        format!("__{}__{}", skill_name, intent_name)
    }
    fn demangle<'a>(&'a self, mangled: &str) -> &'a str {
        self.demangled_names.get(mangled).expect("Mangled name was not found")
    }
    pub fn add_intent(&mut self, sig_arg: IntentData, intent_name: &str, skill_name: &str, act_set: Arc<Mutex<ActionSet>>) -> Result<()> {
        let mangled = Self::mangle(intent_name, skill_name);
        add_intent_to_nlu(&mut self.nlu, sig_arg, &mangled, skill_name, &self.langs)?;
        self.intent_map.add_mapping(&mangled, act_set);
        self.demangled_names.insert(mangled, intent_name.to_string());
        Ok(())
    }

    pub fn add_slot_type(&mut self, type_name: String, data: EntityDef) -> Result<()> {
        for lang in &self.langs {
            let nlu_man= self.nlu.get_mut(lang).expect("Language not registered").get_mut_nlu_man();
            let trans_data = data.clone().into_translation(lang)?;
            nlu_man.add_entity(&type_name, trans_data);
        }
        Ok(())
    }
}


#[async_trait(?Send)]
impl<M:NluManager + NluManagerStatic + NluManagerConf + Debug> Signal for SignalOrder<M> {
    fn end_load(&mut self, _curr_lang: &Vec<LanguageIdentifier>) -> Result<()> {
        self.end_loading()
    }
    async fn event_loop(&mut self, signal_event: SignalEventShared, config: &Config, base_context: &ActionContext, curr_lang: &Vec<LanguageIdentifier>) -> Result<()> {
        self.record_loop(signal_event, config, base_context, curr_lang).await
    }
}

impl YamlEntityDef {
    fn try_into_with_trans(self, lang: &LanguageIdentifier) -> Result<EntityDef> {
        let mut data = Vec::new();

        for trans_data in self.data.into_iter(){
            let mut translations = try_translate_all(&trans_data, &lang.to_string())?;
            let value = translations.swap_remove(0);
            data.push(EntityData{value, synonyms: StringList::from_vec(translations)});
        }

        Ok(EntityDef{data, automatically_extensible: true})
    }
}

#[cfg(not(feature = "devel_rasa_nlu"))]
pub type CurrentNluManager = SnipsNluManager;
#[cfg(feature = "devel_rasa_nlu")]
pub type CurrentNluManager = RasaNluManager;

pub type SignalOrderCurrent = SignalOrder<CurrentNluManager>;

pub fn new_signal_order(langs: Vec<LanguageIdentifier>) -> SignalOrder<CurrentNluManager> {
    SignalOrder::new(langs)
}

