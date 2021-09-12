// Standard library
use std::sync::{Arc, Mutex};

// This crate
use crate::actions::{Action, ActionAnswer, ActionContext, ActionInstance, LocalActionRegistry};
use crate::exts::LockIt;
use crate::queries::LocalQueryRegistry;
use crate::signals::{LocalSignalRegistry, order::mqtt::MSG_OUTPUT};
use crate::skills::Loader;

// Other crates
use anyhow::Result;
use bytes::Bytes;
use rmp_serde::decode;
use rumqttc::{AsyncClient, QoS};
use unic_langid::LanguageIdentifier;

mod messages {
    use serde::Deserialize;

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
        let msg: messages::SayMessage = decode::from_read(std::io::Cursor::new(payload))?;
        MSG_OUTPUT.with::<_,Result<()>>(|m|{match *m.borrow_mut() {
            Some(ref mut output) => {
                // Note: This clone could be workarounded
                let l =msg.lang.and_then(|s|s.parse().ok()).unwrap_or(self.def_lang.clone());
                output.answer(msg.text, &l, msg.site_id)
            }
            _=>{
                Err(anyhow::anyhow!("No output channel"))
            }
        }})
    }
}

pub struct HermesApiOut {
}

impl HermesApiOut {
    pub fn new() -> Self {
        Self {}
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

impl ActionInstance for HermesActionInstance {
    fn call(&self ,context: &ActionContext) -> Result<ActionAnswer> {
        // TODO: What now with ActionAnswers?
        std::unimplemented!("HermesActionInstance call is not implemented");
        //Ok(ActionAnswer::send_text("NYI", true))
    }

    fn get_name(&self) -> String {
        self.name.as_ref().clone()
    }
}