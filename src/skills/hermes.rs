// Standard library
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// This crate
use crate::actions::{Action, ActionAnswer, ActionContext, ACT_REG};
use crate::exts::LockIt;
use crate::signals::{order::mqtt::MSG_OUTPUT, SIG_REG};
use crate::skills::hermes::messages::IntentMessage;
use crate::skills::SkillLoader;

// Other crates
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use bytes::Bytes;
use lazy_static::lazy_static;
use rumqttc::{AsyncClient, QoS};
use serde::Serialize;
use tokio::sync::{mpsc, oneshot};
use unic_langid::LanguageIdentifier;

lazy_static! {
    static ref HERMES_API_OUTPUT: Arc<Mutex<Option<HermesApiOutput>>> = Arc::new(Mutex::new(None));
    static ref HERMES_API_INPUT: Arc<Mutex<Option<HermesApiInput>>> = Arc::new(Mutex::new(None));
}

mod messages {
    use serde::{Deserialize, Serialize};
    use serde_json::Value;

    #[derive(Deserialize)]
    pub struct SayMessage {
        pub text: String,

        #[serde(default)]
        pub lang: Option<String>,

        #[serde(default)]
        pub id: Option<String>,

        #[serde(default)]
        pub volume: Option<f32>,

        #[serde(default = "default_site", rename = "siteId")]
        pub site_id: String,

        #[serde(default, rename = "sessionId")]
        pub session_id: Option<String>,
    }

    #[derive(Serialize)]
    pub struct IntentMessage {
        pub input: String,
        pub intent: ObjectIntentMessage,

        #[serde(default)]
        pub id: Option<String>,

        #[serde(default = "default_site", rename = "siteId")]
        pub site_id: String,

        #[serde(default, rename = "sessionId")]
        pub session_id: Option<String>,

        #[serde(default, rename = "customData")]
        pub custom_data: Option<String>,

        #[serde(default, rename = "asrTokens")]
        pub asr_tokens: Vec<AsrTokenIntentMessage>,

        #[serde(default, rename = "asrConfidence")]
        pub asr_confidence: Option<f32>,
    }

    #[derive(Serialize)]
    pub struct ObjectIntentMessage {
        #[serde(rename = "intentName")]
        pub intent_name: String,
        #[serde(rename = "confidenceScore")]
        pub confidence_score: f32,

        #[serde(default)]
        pub slots: Vec<SlotIntentMessage>,
    }

    #[derive(Serialize)]
    pub struct SlotIntentMessage {
        pub entity: String,
        #[serde(rename = "slotName")]
        pub slot_name: String,
        #[serde(rename = "rawValue")]
        pub raw_value: String,
        pub value: ValueSlotIntentMessage,

        #[serde(default)]
        pub range: Option<RangeSlotIntentMessage>,
    }

    #[derive(Serialize)]
    pub struct ValueSlotIntentMessage {
        pub value: Value, // TODO: This is supposed to be ANY in the definition
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

pub struct HermesLoader {}

impl HermesLoader {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait(?Send)]
impl SkillLoader for HermesLoader {
    fn load_skills(&mut self, _langs: &[LanguageIdentifier]) -> Result<()> {
        // For the time being we are going to put everything as a single skill called "hermes"
        // TODO: Get all intents from somewhere
        let mut act_grd = ACT_REG.lock_it();
        let sig_grd = SIG_REG.lock_it();
        let sig_order = sig_grd
            .get_sig_order()
            .expect("Order signal was not initialized");

        let intents: Vec<String> = Vec::new();
        for intent_name in intents {
            let arc_intent_name = Arc::new(intent_name.clone());
            let action = Arc::new(Mutex::new(HermesAction::new(
                arc_intent_name.clone(),
                arc_intent_name,
            )));
            act_grd.insert("hermes", &intent_name, action.clone())?;
            // TODO! Add to sig order!
        }

        Ok(())
    }

    async fn run_loader(&mut self) -> Result<()> {
        Ok(())
    }
}

pub struct HermesApiIn {
    def_lang: LanguageIdentifier,
}

impl HermesApiIn {
    pub fn new(def_lang: LanguageIdentifier) -> Self {
        Self { def_lang }
    }

    pub async fn subscribe(client: &Arc<Mutex<AsyncClient>>) -> Result<()> {
        let client_raw = client.lock_it();
        client_raw
            .subscribe("hermes/tts/say", QoS::AtLeastOnce)
            .await?;

        Ok(())
    }

    pub async fn handle_tts_say(&self, payload: &Bytes) -> Result<()> {
        if let Ok(Some(msg)) = HERMES_API_INPUT
            .lock_it()
            .as_mut()
            .expect("No Hermes API input")
            .intercept_tts_say(payload)
        {
            MSG_OUTPUT.with::<_, Result<()>>(|m| match *m.borrow_mut() {
                Some(ref mut output) => {
                    let l = msg
                        .lang
                        .and_then(|s| s.parse().ok())
                        .unwrap_or_else(|| self.def_lang.clone());
                    output.answer(msg.text, &l, msg.site_id)
                }
                _ => Err(anyhow!("No output channel")),
            })
        } else {
            Ok(())
        }
    }
}

pub struct HermesApiInput {
    tts_say_map: HashMap<String, oneshot::Sender<messages::SayMessage>>,
}

impl HermesApiInput {
    pub async fn wait_answer(&mut self, uuid: &str) -> messages::SayMessage {
        let (sender, mut receiver) = oneshot::channel();
        self.tts_say_map.insert(uuid.to_string(), sender);
        receiver.try_recv().expect("TTS Say channel dropped")
    }

    pub fn intercept_tts_say(&mut self, msg: &Bytes) -> Result<Option<messages::SayMessage>> {
        let msg: messages::SayMessage = serde_json::from_reader(std::io::Cursor::new(msg))?;
        if let Some(s) = self.tts_say_map.remove(&msg.site_id) {
            s.send(msg).map_err(|_| anyhow!("TTS Say channel error"))?;
            Ok(None)
        } else {
            Ok(Some(msg))
        }
    }
}

pub struct HermesApiOut {
    common_out: mpsc::Receiver<(String, String)>,
}

impl HermesApiOut {
    pub fn new() -> Result<Self> {
        let (sender, common_out) = mpsc::channel(100);
        let output = HermesApiOutput::new(sender);
        HERMES_API_OUTPUT.lock_it().replace(output);

        Ok(Self { common_out })
    }

    pub async fn handle_out(&mut self, client: &Arc<Mutex<AsyncClient>>) -> Result<()> {
        loop {
            let (topic, payload) = self.common_out.recv().await.expect("Out channel broken");
            client
                .lock_it()
                .publish(topic, QoS::AtMostOnce, false, payload)
                .await?;
        }
    }
}

pub struct HermesApiOutput {
    client: mpsc::Sender<(String, String)>,
}

impl HermesApiOutput {
    pub fn new(client: mpsc::Sender<(String, String)>) -> Self {
        Self { client }
    }

    pub fn send<M: Serialize>(&self, path: String, msg: &M) -> Result<()> {
        let msg_str = serde_json::to_string(msg)?;
        self.client.try_send((path, msg_str)).unwrap();
        Ok(())
    }
}

pub struct HermesAction {
    name: Arc<String>,
    intent_name: Arc<String>,
}

impl HermesAction {
    pub fn new(name: Arc<String>, intent_name: Arc<String>) -> Self {
        Self { name, intent_name }
    }
}

#[async_trait(?Send)]
impl Action for HermesAction {
    async fn call(&mut self, context: &ActionContext) -> Result<ActionAnswer> {
        const ERR: &str = "DynamicDict lacks mandatory element";

        let intent_name = (*self.intent_name).clone();
        let intent_data = context.data.as_intent().expect(ERR);
        let msg = IntentMessage {
            id: None,
            input: intent_data.input.clone(),
            intent: messages::ObjectIntentMessage {
                intent_name: intent_name.clone(),
                confidence_score: 1.0,
                slots: intent_data
                    .slots
                    .iter()
                    .map(|(n, v)| {
                        messages::SlotIntentMessage {
                            raw_value: v.clone(),
                            value: messages::ValueSlotIntentMessage {
                                value: serde_json::Value::String(v.clone()),
                            },
                            entity: n.to_string(),
                            slot_name: n.clone(),
                            range: None, // TODO: Actually get to pass this information
                        }
                    })
                    .collect(),
            },
            site_id: context.satellite.as_ref().expect(ERR).uuid.clone(),
            session_id: None,
            custom_data: None,
            asr_tokens: vec![],
            asr_confidence: None,
        };
        HERMES_API_OUTPUT
            .lock_it()
            .as_ref()
            .unwrap()
            .send(format!("/hermes/intent/{}", intent_name), &msg)?;
        let msg = HERMES_API_INPUT
            .lock_it()
            .as_mut()
            .unwrap()
            .wait_answer("/hermes/tts/say")
            .await;

        // TODO: Check that it is for the same site

        // TODO: Only end session if requested
        ActionAnswer::send_text(msg.text, true)
    }

    fn get_name(&self) -> String {
        self.name.as_ref().clone()
    }
}
