pub mod server_interface;

// Standard library
use std::collections::HashMap;
use std::fmt::{self, Debug};
use std::mem::replace;
use std::sync::{Arc, Mutex};

// This crate
use crate::actions::{ActionAnswer, ActionContext, ActionSet, MainAnswer};
use crate::config::Config;
use crate::nlu::{EntityData, EntityDef, EntityInstance, Nlu, NluManager, NluManagerConf, NluManagerStatic, NluResponseSlot, NluUtterance};
use crate::python::{try_translate, try_translate_all};
use crate::stt::DecodeRes;
use crate::signals::{ActMap, Signal, SignalEventShared};
use crate::vars::{mangle, MIN_SCORE_FOR_ACTION, POISON_MSG};
use self::server_interface::{MqttInterface, MSG_OUTPUT, on_event, on_nlu_request, SessionManager};

// Other crates
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use lazy_static::lazy_static;
use log::{debug, info, error, warn};
use serde::{Deserialize, Deserializer, de::{self, SeqAccess, Visitor}, Serialize, Serializer, ser::SerializeSeq};
use tokio::{select, sync::mpsc};
use unic_langid::LanguageIdentifier;

#[cfg(not(feature = "devel_rasa_nlu"))]
use crate::nlu::SnipsNluManager;

#[cfg(feature = "devel_rasa_nlu")]
use crate::nlu::RasaNluManager;

lazy_static! {
    pub static ref ENTITY_ADD_CHANNEL: Mutex<Option<mpsc::Sender<EntityAddValueRequest>>> =  Mutex::new(None);
}

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


#[derive(Clone, Debug)]
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
        formatter.write_str("either a string or a list containing strings")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E> where E: de::Error{
        Ok(StringList{data:vec![v.to_string()]})
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E> where E: de::Error{
        Ok(StringList{data:vec![v]})
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
        deserializer.deserialize_any(StringListVisitor)
    }
}

// Serialize this as a list of strings
impl Serialize for StringList {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,{
        let mut seq = serializer.serialize_seq(Some(self.data.len()))?;
        for e in &self.data {
            seq.serialize_element(e)?;
        }
        seq.end()
    }
}



#[derive(Debug, Deserialize)]
struct SlotData {
    #[serde(rename="type")]
    slot_type: OrderKind,
    #[serde(default="false_val")]
    required: bool,
    #[serde(default="none")]
    prompt: Option<String>,
    #[serde(default="none")]
    reprompt: Option<String>
}
fn false_val() -> bool {false}
fn none() -> Option<String> {None}
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

fn add_intent_to_nlu<N: NluManager + NluManagerStatic + NluManagerConf + Send>(
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
pub struct NluState<M: NluManager + NluManagerStatic + NluManagerConf + Send> {
    manager: M,
    nlu: Option<M::NluType>
}

impl<M: NluManager + NluManagerStatic + NluManagerConf + Send> NluState<M> {
    fn get_mut_nlu_man(&mut self) -> &mut M {
        &mut self.manager
    }

    fn new(manager: M) -> Self {
        Self {manager, nlu: None}
    }
}

#[derive(Debug)]
pub struct SignalOrder<M: NluManager + NluManagerStatic + NluManagerConf + Debug + Send> {
    intent_map: ActMap,
    nlu: Arc<Mutex<HashMap<LanguageIdentifier, NluState<M>>>>,
    demangled_names: HashMap<String, String>,
    dyn_entities: Option<mpsc::Receiver<EntityAddValueRequest>>,
    langs: Vec<LanguageIdentifier>
}

impl<M:NluManager + NluManagerStatic + NluManagerConf + Debug + Send + 'static> SignalOrder<M> {
    pub fn new(langs: Vec<LanguageIdentifier>, consumer: mpsc::Receiver<EntityAddValueRequest>) -> Self {
        let mut managers = HashMap::new();
        for lang in &langs {
            managers.insert(lang.to_owned(), NluState::new(M::new()));
        }
        SignalOrder {
            intent_map: ActMap::new(),
            nlu: Arc::new(Mutex::new(managers)),
            demangled_names: HashMap::new(),
            dyn_entities: Some(consumer),
            langs
        }
    }

    pub fn end_loading(nlu: Arc<Mutex<HashMap<LanguageIdentifier, NluState<M>>>>,langs: &Vec<LanguageIdentifier>) -> Result<()> {
        for lang in langs {
            let (train_path, model_path) = M::get_paths();
            let err = || {anyhow!("Received language '{}' has not been registered", lang.to_string())};
            let mut m = nlu.lock().expect(POISON_MSG);
            let nlu  = m.get_mut(lang).ok_or_else(err)?;
            if M::is_lang_compatible(lang) {
                nlu.manager.ready_lang(lang)?;
                nlu.nlu = Some(nlu.manager.train(&train_path, &model_path.join("main_model.json"), lang)?);
            }
            else {
                Err(anyhow!("{} NLU is not compatible with the selected language", M::name()))?
            }

            info!("Initted Nlu");
        }

        Ok(())
    }

    pub async fn received_order(&mut self, decode_res: Option<DecodeRes>, event_signal: SignalEventShared, base_context: &ActionContext, lang: &LanguageIdentifier, satellite: String) -> Result<bool> {
        debug!("Heard from user: {:?}", decode_res);

        let ans = match decode_res {
            None => event_signal.lock().expect(POISON_MSG).call("empty_reco", base_context.clone()),
            Some(decode_res) => {

                if !decode_res.hypothesis.is_empty() {
                    const ERR_MSG: &str = "Received language to the NLU was not registered";
                    const NO_NLU_MSG: &str = "received_order can't be called before end_loading";
                    let mut m = self.nlu.lock().expect(POISON_MSG);
                    let nlu = &m.get_mut(&lang).expect(ERR_MSG).nlu.as_mut().expect(NO_NLU_MSG);
                    let result = nlu.parse(&decode_res.hypothesis).await.map_err(|err|anyhow!("Failed to parse: {:?}", err))?;
                    info!("{:?}", result);

                    // Do action if at least we are 80% confident on
                    // what we got
                    if result.confidence >= MIN_SCORE_FOR_ACTION {
                        if let Some(intent_name) = result.name {
                            info!("Let's call an action");

                            let slots_data =  add_slots(&ActionContext::new(),result.slots);

                            let mut intent_data = ActionContext::new();
                            intent_data.set_str("name".to_string(), self.demangle(&intent_name).to_string());
                            intent_data.set_dict("slots".to_string(), slots_data);

                            let mut intent_context = base_context.clone();
                            intent_context.set_str("type".to_string(), "intent".to_string());
                            intent_context.set_dict("intent".to_string(), intent_data);
                            
                            let answers = self.intent_map.call_mapping(&intent_name, &intent_context);
                            info!("Action called");
                            answers
                        }
                        else {
                            event_signal.lock().expect(POISON_MSG).call("unrecognized", base_context.clone())
                        }
                    }
                    else {
                        event_signal.lock().expect(POISON_MSG).call("unrecognized", base_context.clone())
                    }
                }
                else {
                    event_signal.lock().expect(POISON_MSG).call("empty_reco", base_context.clone())
                }
            }
        };
        
        process_answers(ans, lang, satellite.clone())
    }

    pub async fn record_loop(&mut self, signal_event: SignalEventShared, config: &Config, base_context: &ActionContext, curr_langs: &Vec<LanguageIdentifier>) -> Result<()> {
        let mut interface = MqttInterface::new()?;

        // Dyn entities data
        
        async fn on_dyn_entity<M: NluManager + NluManagerConf + NluManagerStatic + Debug + Send + 'static>(
            mut channel: mpsc::Receiver<EntityAddValueRequest>,
            shared_nlu: Arc<Mutex<HashMap<LanguageIdentifier, NluState<M>>>>,
            curr_langs: Vec<LanguageIdentifier>,
        ) -> Result<()> {
            loop {
                let request= channel.recv().await.unwrap();
                let langs = if request.langs.is_empty(){
                    curr_langs.clone()
                }
                else {
                    request.langs
                };

                let mut m = shared_nlu.lock().expect(POISON_MSG);
                for lang in langs {
                    let man = m.get_mut(&lang).expect("Language not registered").get_mut_nlu_man();
                    let mangled = mangle(&request.skill, &request.entity);
                    if let Err(e) = man.add_entity_value(&mangled, request.value.clone()) {
                        error!("Failed to add value to entity {}", e);
                    }

                    SignalOrder::end_loading(shared_nlu.clone(), &curr_langs)?;
                }
            }
        }

        let (nlu_sender, nlu_receiver) = tokio::sync::mpsc::channel(100);
        let (event_sender, event_receiver) = tokio::sync::mpsc::channel(100);
        let sessions = Arc::new(Mutex::new(SessionManager::new()));
        let def_lang = curr_langs.get(0);
        let dyn_ent_fut = on_dyn_entity(
             replace(&mut self.dyn_entities, None).expect("Dyn_entities already consumed"),
             self.nlu.clone(),
             curr_langs.clone()
        );
        select!{
            e = dyn_ent_fut => {Err(anyhow!("Dynamic entitying failed: {:?}",e))}
            e = interface.interface_loop(config, curr_langs, def_lang, sessions.clone(), nlu_sender, event_sender) => {e}
            e = on_nlu_request(config, nlu_receiver, signal_event.clone(), curr_langs, self, base_context, sessions) => {Err(anyhow!("Nlu request failed: {:?}", e))}
            e = on_event(event_receiver, signal_event, def_lang, base_context) => {Err(anyhow!("Event handling failed: {:?}", e))}
        }
    }
}


fn add_slots(base_context: &ActionContext, slots: Vec<NluResponseSlot>) -> ActionContext {
    let mut result = base_context.clone();
    for slot in slots.into_iter() {
        result.set_str(slot.name, slot.value);
    }

    result
}

fn process_answer(ans: ActionAnswer, lang: &LanguageIdentifier, uuid: String) -> Result<()> {
    MSG_OUTPUT. with::<_,Result<()>>(|m|{match *m.borrow_mut() {
        Some(ref mut output) => {
            match ans.answer {
                MainAnswer::Sound(s) => {
                    output.send_audio(s, uuid)
                }
                MainAnswer::Text(t) => {
                    output.answer(t, lang, uuid)
                }
            }?;  
        }
        _=>{}
    };
    Ok(())
    })
}

pub fn process_answers(
    ans: Option<Vec<ActionAnswer>>,
    lang: &LanguageIdentifier,
    satellite: String) -> Result<bool> {
    if let Some(answers) = ans {
        let (ans,errs): (Vec<_>, Vec<Result<bool>>) = answers.into_iter()
        .map(|a|{
            let s = a.should_end_session;
            process_answer(a, lang, satellite.clone())?;
            Ok(s)
        })
        .partition(Result::is_err);

        errs.into_iter()
        .map(|e|e.err().unwrap())
        .for_each(|e| error!("There was an error while handling answer: {}", e));

        // By default, the session is ended, it is kept alive as soon as someone asks not to do that
        let s = ans.into_iter().map(|s|s.ok().unwrap()).fold(true,|a,b|{!(!a|!b)});
        Ok(s)
    }
    else {
        // Should end session?
        Ok(true)
    }

}

impl<M:NluManager + NluManagerStatic + NluManagerConf + Debug + Send>  SignalOrder<M> {
    fn demangle<'a>(&'a self, mangled: &str) -> &'a str {
        self.demangled_names.get(mangled).expect("Mangled name was not found")
    }
    pub fn add_intent(&mut self, sig_arg: IntentData, intent_name: &str, skill_name: &str, act_set: Arc<Mutex<ActionSet>>) -> Result<()> {
        let mangled = mangle(skill_name, intent_name);
        add_intent_to_nlu(&mut self.nlu.lock().expect(POISON_MSG), sig_arg, &mangled, skill_name, &self.langs)?;
        self.intent_map.add_mapping(&mangled, act_set);
        self.demangled_names.insert(mangled, intent_name.to_string());
        Ok(())
    }

    pub fn add_slot_type(&mut self, type_name: String, data: EntityDef) -> Result<()> {
        for lang in &self.langs {
            let mut m = self.nlu.lock().expect(POISON_MSG);
            let nlu_man= m.get_mut(lang).expect("Language not registered").get_mut_nlu_man();
            let trans_data = data.clone().into_translation(lang)?;
            nlu_man.add_entity(&type_name, trans_data);
        }
        Ok(())
    }
}


#[async_trait(?Send)]
impl<M:NluManager + NluManagerStatic + NluManagerConf + Debug + Send + 'static> Signal for SignalOrder<M> {
    fn end_load(&mut self, _curr_lang: &Vec<LanguageIdentifier>) -> Result<()> {
        Self::end_loading(self.nlu.clone(), &self.langs)
    }
    async fn event_loop(&mut self, signal_event: SignalEventShared, config: &Config, base_context: &ActionContext, curr_lang: &Vec<LanguageIdentifier>) -> Result<()> {
        self.record_loop(signal_event, config, base_context, curr_lang).await
    }
}

pub struct EntityAddValueRequest {
    pub skill: String,
    pub entity: String,
    pub value: String,
    pub langs: Vec<LanguageIdentifier>,
}
pub fn init_dynamic_entities() -> Result<mpsc::Receiver<EntityAddValueRequest>> {
    let (producer, consumer) = mpsc::channel(100);

    (*ENTITY_ADD_CHANNEL.lock().expect(POISON_MSG)) = Some(producer);

    Ok(consumer)
}

impl YamlEntityDef {
    fn try_into_with_trans(self, lang: &LanguageIdentifier) -> Result<EntityDef> {
        let mut data = Vec::new();

        for trans_data in self.data.into_iter(){
            let mut translations = try_translate_all(&trans_data, &lang.to_string())?;
            let value = translations.swap_remove(0);
            data.push(EntityData{value, synonyms: StringList::from_vec(translations)});
        }

        Ok(EntityDef::new(data, true))
    }
}

#[cfg(not(feature = "devel_rasa_nlu"))]
pub type CurrentNluManager = SnipsNluManager;
#[cfg(feature = "devel_rasa_nlu")]
pub type CurrentNluManager = RasaNluManager;

pub type SignalOrderCurrent = SignalOrder<CurrentNluManager>;

pub fn new_signal_order(langs: Vec<LanguageIdentifier>, consumer: mpsc::Receiver<EntityAddValueRequest>) -> SignalOrder<CurrentNluManager> {
    SignalOrder::new(langs, consumer)
}

