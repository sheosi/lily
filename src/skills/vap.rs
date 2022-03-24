// Standard library
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// This crate
use crate::actions::{Action, ActionAnswer, ActionContext};
use crate::nlu::{IntentData, OrderKind, SlotData};
use crate::signals::collections::Hook;
use crate::skills::{register_skill, SkillLoader};

// Other crates
use anyhow::Result;
use async_trait::async_trait;
use unic_langid::LanguageIdentifier;
use vap_common_skill::structures::msg_skill_request::{ClientData, RequestData, RequestSlot};
use vap_common_skill::structures::{MsgRegisterIntents, MsgRegisterIntentsResponse, MsgSkillRequest, Language};
use vap_skill_register::{SkillRegister, SkillRegisterMessage, SkillRegisterStream, Response, ResponseType};

// TODO: Move this into config
mod conf {
    pub const PORT: u16 = 5683;
}

pub struct VapLoader {
}

impl VapLoader {
    pub fn new() -> Self {
        VapLoader {

        }
    }

    async fn on_msg(&mut self, mut stream: SkillRegisterStream) -> Result<(), vap_skill_register::Error> {
        loop {
            let (msg, responder) = stream.recv().await?;

            let response = match msg {
                SkillRegisterMessage::Connect(msg) => {
                    // TODO!
                    Response {
                        status: ResponseType::Valid,
                        payload: vec![]
                    }
                }

                SkillRegisterMessage::RegisterIntents(msg) => {
                    match register_skill("TODO! Add name of client", Self::transform(msg), vec![], vec![]) {
                        Ok(()) => {
                            Response {
                                status: ResponseType::Valid,
                                payload: vec![]
                            }
                        }

                        Err(_) => {
                            Response {
                                status: ResponseType::RequestEntityIncomplete,
                                // TODO! Add why
                                payload: vec![]
                            }
                        }
                    }
                }

                SkillRegisterMessage::Close(_) => {
                    // TODO!
                    Response {
                        status: ResponseType::Valid,
                        payload: vec![]
                    }
                }

                _ => {
                    Response {
                        status: ResponseType::NotImplemented,
                        payload: vec![]
                    }
                }
            };

            responder.send(response).map_err(|_| vap_skill_register::Error::ClosedChannel)?;
        }
    }

    fn transform(msg: MsgRegisterIntents) -> Vec<(String, HashMap<LanguageIdentifier, IntentData>, Arc<Mutex<dyn Action + Send>>)> {
        let new_intents: HashMap<String, HashMap<LanguageIdentifier, IntentData>> = HashMap::new();

        fn fmt_name(name: &str) -> String {
            format!("vap_action_{}", name)
        }

        for lang_set in msg.nlu_data.into_iter() {
            for intent in lang_set.intents {
        
                let internal_intent = IntentData {
                    slots: intent.slots.into_iter().map(|s|(s.name.clone(), SlotData {
                        slot_type: OrderKind::Ref(s.entity),
                        required: false,
                        prompt: None,
                        reprompt: None,
                    })).collect(),
                    utts: intent.utterances.into_iter().map(|d|d.text).collect(), // TODO! Utterances might need some conversion of slot format
                    hook: Hook::Action(fmt_name(&intent.name)),
                };
            }

            for entity in lang_set.entities {

            }
        }

        new_intents.into_iter().map(
            |(intent, utts)| {
            let action: Arc<Mutex<dyn Action + Send>> = Arc::new(
                Mutex::new(
                    VapAction::new(
                        fmt_name(&intent),
                        "TODO!Figure ip".to_string()
                    )
                )
            );
            (intent, utts, action)
        }).collect()
    }
}

#[async_trait(?Send)]
impl SkillLoader for VapLoader {
    fn load_skills(&mut self, _langs: &Vec<LanguageIdentifier>) -> Result<()> {
        Ok(())
    }
    
    async fn run_loader(&mut self) -> Result<()> {
        let (reg, stream) = SkillRegister::new("test-skill-register", conf::PORT).unwrap();
        tokio::select!(
            _= reg.run() => {},
            _= self.on_msg(stream) => {}
        );

        Ok(())
    }
}

struct VapAction {
    name: String,
    ip: String
}

impl VapAction {
    pub fn new(name: String, ip: String) -> Self {
        VapAction {name, ip}
    }
}

#[async_trait(?Send)]
impl Action for VapAction {
    async fn call(&self, context: &ActionContext) -> Result<ActionAnswer> {
        // TODO! Also map events!!!
        let slots = context.data.as_intent()
            .unwrap()
            .slots.iter()
            .map(|(n,v)| RequestSlot {
                name: n.clone(),
                value: Some(v.clone())
            }).collect();

        let resp = SkillRegister::activate_skill(self.ip.clone(), MsgSkillRequest {
            client: ClientData {
                system_id: context.satellite.as_ref().map(|s|s.uuid.clone()).expect("No satellite"),
                capabilities: vec![], // TODO! Figure out capabilities
            },
            request: RequestData {
                type_: "intent".to_string(),
                intent: context.data.as_intent().unwrap().name.clone(),
                locale: context.locale.clone(),
                slots
            }
        }).await?;

        let data = if resp.capabilities[0].name == "voice" {
            resp.capabilities[0].data.clone()
        } else {
            "NO VOICE CAPBILITY IN RESPONSE".to_string()
        };
        ActionAnswer::send_text(data, true)
    }

    fn get_name(&self) -> String {
        self.name.clone()
    }
}