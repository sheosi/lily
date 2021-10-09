// Standard library
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// This crate
use crate::actions::{Action, ActionAnswer, ActionContext, ActionInstance, LocalActionRegistry};
use crate::exts::LockIt;
use crate::queries::LocalQueryRegistry;
use crate::signals::{LocalSignalRegistry, order::mqtt::MSG_OUTPUT};
use crate::skills::Loader;
use crate::skills::hermes::messages::IntentMessage;

// Other crates
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use bytes::Bytes;
use lazy_static::lazy_static;
use rmp_serde::decode;
use rumqttc::{AsyncClient, QoS};
use serde::Serialize;
use tokio::sync::{mpsc, oneshot};
use unic_langid::LanguageIdentifier;

lazy_static!{
    static ref HERMES_API_OUTPUT: Arc<Mutex<Option<HermesApiOutput>>> = Arc::new(Mutex::new(None));
    static ref HERMES_API_INPUT: Arc<Mutex<Option<HermesApiInput>>> = Arc::new(Mutex::new(None));
}

//TODO! XXX: In order for us to properly pipe the Action Answer, we need to intercept the inbound message
//TODO! XXX: For that we need HermesApiInput and make it async, we need to make call (and most possibly it's trait)
//TODO! XXX: async too.
mod messages {
    use serde::{Deserialize, Serialize};

    #[derive(Deserialize)]
    pub struct SayMessage {
        pub text: String,

        #[serde(default)]
        pub lang: Option<String>,

        #[serde(default)]
        pub id: Option<String>,

        #[serde(default)]
        pub volume: Option<f32>,

        #[serde(default="default_site", rename="siteId")]
        pub site_id: String,

        #[serde(default, rename="sessionId")]
        pub session_id: Option<String>
    }

    #[derive(Serialize)]
    pub struct IntentMessage {
        pub input: String,
        pub intent: ObjectIntentMessage,

        #[serde(default)]
        pub id: Option<String>,

        #[serde(default="default_site", rename="siteId")]
        pub site_id: String,

        #[serde(default, rename="sessionId")]
        pub session_id: Option<String>,

        #[serde(default, rename="customData")]
        pub custom_data: Option<String>,

        #[serde(default, rename="asrTokens")]
        pub asr_tokens: Vec<AsrTokenIntentMessage>,

        #[serde(default, rename="asrConfidence")]
        pub asr_confidence: Option<f32>
    }

    #[derive(Serialize)]
    pub struct ObjectIntentMessage {
        intentName: String,
        confidenceScore: f32,

        #[serde(default)]
        slots: Vec<SlotIntentMessage>,
    }

    #[derive(Serialize)]
    pub struct SlotIntentMessage {
        entity: String,
        slotName: String,
        rawValue: String,
        value: ValueSlotIntentMessage,

        #[serde(default)]
        range: Option<RangeSlotIntentMessage>,
    }

    #[derive(Serialize)]
    pub struct ValueSlotIntentMessage {
        pub value: String, // TODO: This is supposed to be ANY in the definition
    }

    #[derive(Serialize)]
    pub struct RangeSlotIntentMessage {
        start: i32,
        end: i32,
    }

    #[derive(Serialize)]
    pub struct AsrTokenIntentMessage {
        value: String,
        confidence: f32,
        range_start: i32,
        range_end: i32,
    }

    fn default_site() -> String {
        "default".into()
    }
}

pub struct HermesLoader {

}

impl HermesLoader {
    pub fn new() -> Self {
        Self {}
    }
}

impl Loader for HermesLoader {
    fn load_skills(&mut self,
        _base_sigreg: &LocalSignalRegistry,
        _base_actreg: &LocalActionRegistry,
        _base_queryreg: &LocalQueryRegistry,
        _langs: &Vec<LanguageIdentifier>) -> Result<()> {


        Ok(())
    }
}

pub struct HermesApiIn {
    def_lang: LanguageIdentifier,
}

impl HermesApiIn {
    pub fn new(def_lang: LanguageIdentifier) -> Self {
        Self {def_lang}
    }

    pub async fn subscribe(client: &Arc<Mutex<AsyncClient>>) -> Result<()> {
        let client_raw = client.lock_it();
        client_raw.subscribe("hermes/tts/say", QoS::AtLeastOnce).await?;

        Ok(())
    }

    pub async fn handle_tts_say(&self, payload: &Bytes) -> Result<()> {
        if let Some(msg) = HERMES_API_INPUT.lock_it().expect("No Hermes API input").intercept_tts_say(payload) {
            MSG_OUTPUT.with::<_,Result<()>>(|m|{match *m.borrow_mut() {
                Some(ref mut output) => {
                    // Note: This clone could be workarounded
                    let l =msg.lang.and_then(|s|s.parse().ok()).unwrap_or(self.def_lang.clone());
                    output.answer(msg.text, &l, msg.site_id)
                }
                _=>{
                    Err(anyhow!("No output channel"))
                }
            }})
        }
        else {Ok(())}
    }
}

pub struct HermesApiInput {
    tts_say_map: HashMap<String, oneshot::Sender<messages::SayMessage>>,
}

impl HermesApiInput {
    pub async fn wait_answer(&mut self, uuid: &str) -> String {
        let (sender, receiver) = oneshot::channel();
        self.tts_say_map.insert(uuid.to_string(), sender);
        let ans = receiver.recv().await.expect("TTS Say channel dropped");
        self.tts_say_map.remove(uuid);
        ans
    }

    pub fn intercept_tts_say(&mut self, msg: Bytes) -> Option<messages::SayMessage> {
        let msg: messages::SayMessage = serde_json::from_str(&msg);
        if self.tts_say_map.contains_key(&msg.site_id) {
            self.tts_say_map[&msg.site_id].send(msg);
            None
        }
        else {
            Some(msg)
        }
    }
}

pub struct HermesApiOut {
    common_out: mpsc::Receiver<(String, String)>
}

impl HermesApiOut {
    pub fn new() -> Result<Self> {
        let (sender, common_out) = mpsc::channel(100);
        let output = HermesApiOutput::new(sender);
        HERMES_API_OUTPUT.lock_it().replace(output);

        Ok(Self {common_out})
    }
}

pub struct HermesApiOutput {
    client: mpsc::Sender<(String, String)>
}

impl HermesApiOutput {
    pub fn new(client: mpsc::Sender<(String, String)>) -> Self {
        Self{client}
    }

    pub fn send<M: Serialize>(&self, path: String,msg: &M) -> Result<()> {
        let msg_str = serde_json::to_string(msg)?;
        self.client.try_send((path, msg_str)).unwrap();
        Ok(())
    }
}

pub struct HermesAction {
    name: Arc<String>,
    intent_name: Arc<String>
}

impl HermesAction {
    pub fn new(name: Arc<String>, intent_name: Arc<String>) -> Self {
        Self {name, intent_name}
    }
}

impl Action for HermesAction {
    fn instance(&self) -> Box<dyn ActionInstance + Send> {
        Box::new(HermesActionInstance::new(self.name.clone(), self.intent_name.clone()))
    }
}

pub struct HermesActionInstance {
    name: Arc<String>,
    intent_name: Arc<String>
}

impl HermesActionInstance {
    pub fn new(name: Arc<String>, intent_name: Arc<String>) -> Self {
        Self {name, intent_name}
    }
}

#[async_trait(?Send)]
impl ActionInstance for HermesActionInstance {
    async fn call(&self ,context: &ActionContext) -> Result<ActionAnswer> {
        const ERR: &str = "ActionContext lacks mandatory element";
        
        let intent_name = (*self.intent_name).clone();
        let msg = IntentMessage {
            id: None,
            input: context.get("intent").expect(ERR).as_dict().unwrap().get("input").expect(ERR).as_string().unwrap().to_string(),
            intent: messages::ObjectIntentMessage {
                intentName: intent_name.clone(),
                confidenceScore: 1.0,
                slots: vec![]
            },
            site_id: context.get("satellite").expect(ERR).as_dict().unwrap().get("uuid").expect(ERR).as_string().unwrap().to_string(),
            session_id: None,
            custom_data: None,
            asr_tokens: vec![],
            asr_confidence: None
            
        };
        HERMES_API_OUTPUT.lock_it().as_ref().unwrap().send(format!("/hermes/intent/{}", intent_name), &msg);
        let msg_str: String = HERMES_API_INPUT.lock_it().as_mut().unwrap().wait_answer("/hermes/tts/say").await?;
        let msg: messages::SayMessage = serde_json::from_str(&msg_str)?;

        // TODO: Check that it is for the same site

        // TODO: Only end session if requested
        ActionAnswer::send_text(msg.text, true)
    }

    fn get_name(&self) -> String {
        self.name.as_ref().clone()
    }
}