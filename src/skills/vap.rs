// Standard library
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

// This crate
use crate::actions::{Action, ActionAnswer, ActionContext};
use crate::exts::LockIt;
use crate::nlu::{IntentData, OrderKind, SlotData, EntityDef, EntityData};
use crate::signals::collections::Hook;
use crate::skills::{register_skill, SkillLoader};

// Other crates
use anyhow::Result;
use async_trait::async_trait;
use rmp_serde::to_vec_named;
use unic_langid::{LanguageIdentifier, subtags};
use vap_common_skill::structures::msg_register_intents::NluData;
use vap_common_skill::structures::msg_skill_request::{ClientData, RequestData, RequestSlot, RequestDataKind};
use vap_common_skill::structures::{MsgRegisterIntentsResponse, MsgSkillRequest, Language, MsgConnectResponse, MsgNotificationResponse};
use vap_skill_register::{SkillRegister, SkillRegisterMessage, SkillRegisterOut, SkillRegisterStream, Response, ResponseType, RequestResponse};

pub struct VapLoader {
    out: Arc<Mutex<SkillRegisterOut>>,
    stream_reg: Option<(SkillRegisterStream, SkillRegister)>,
    langs: Vec<Language>
}

fn to_lang_struct(l: LanguageIdentifier) -> Language {
    Language {
        country: l.region.map(|r|r.to_string()),
        language: l.language.to_string(),
        extra: l.script.map(|s|s.to_string())
    }
}

impl VapLoader {
    pub fn new(port: u16, langs: Vec<LanguageIdentifier>) -> Self {
        let (reg, stream, out) = SkillRegister::new("test-skill-register", port).unwrap();
        let langs = langs.into_iter().map(to_lang_struct).collect();

        VapLoader {
            out: Arc::new(Mutex::new(out)),
            stream_reg: Some((stream, reg)),
            langs
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
                        payload: to_vec_named(&MsgConnectResponse{
                            langs: self.langs.clone(),
                            unique_authentication_token: Some("".into()) // TODO: Finish security
                        }).unwrap()
                    }
                }

                SkillRegisterMessage::RegisterIntents(msg) => {
                    let (actions, entities) = self.transform(msg.nlu_data);
                    match register_skill(&msg.skill_id, actions, vec![], vec![], entities) {
                        Ok(()) => {
                            Response {
                                status: ResponseType::Valid,
                                payload: to_vec_named(&MsgRegisterIntentsResponse{
                                    
                                }).unwrap()
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
                    // TODO! We should unregister the skill from the NLU
                    Response {
                        status: ResponseType::Valid,
                        payload: Vec::new()
                    }
                }

                SkillRegisterMessage::Notification(_) => {
                    // TODO!
                    Response {
                        status: ResponseType::Valid,
                        payload: to_vec_named(&MsgNotificationResponse {
                            data: Vec::new() // TODO!
                        }).unwrap()
                    }
                }

                SkillRegisterMessage::Query(msg) => {
                    // TODO!
                    Response {
                        status: ResponseType::Valid,
                        payload: vec![]
                    }
                }
            };

            responder.send(response).map_err(|_| vap_skill_register::Error::ClosedChannel)?;
        }
    }

    fn transform(&self, nlu_data: Vec<NluData>) -> 
        (
            Vec<(String, HashMap<LanguageIdentifier, IntentData>, Arc<Mutex<dyn Action + Send>>)>,
            Vec<(String, HashMap<LanguageIdentifier, EntityDef>)>,
        ) {
        let mut new_intents: HashMap<String, HashMap<LanguageIdentifier, IntentData>> = HashMap::new();
        let mut entities: HashMap<String, HashMap<LanguageIdentifier, EntityDef>> = HashMap::new();

        fn fmt_name(name: &str) -> String {
            format!("vap_action_{}", name)
        }

        fn fmt_lang(lang: Language) -> LanguageIdentifier {
            LanguageIdentifier::from_parts(
                subtags::Language::from_str(&lang.language).unwrap(),
                Option::None,
                lang.country.and_then(|r| subtags::Region::from_str(&r).ok()),
                &lang.extra.and_then(|e|subtags::Variant::from_str(&e).ok()).map(|v|vec![v]).unwrap_or_else(Vec::new)
            )
        }

        for lang_set in nlu_data.into_iter() {
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

                if !new_intents.contains_key(&intent.name) {
                    new_intents.insert(intent.name.clone(), HashMap::new());
                }
            
                assert!(
                    new_intents.get_mut(&intent.name).unwrap()
                    .insert(fmt_lang(lang_set.language.clone()), internal_intent)
                    .is_none()
                );
            }

            for entity in lang_set.entities {
                let def = EntityDef {
                    data: entity.data.into_iter().map(|d|EntityData {
                        value: d.value,
                        synonyms: d.synonyms
                    }).collect(),
                    automatically_extensible: !entity.strict
                };
                
                if !entities.contains_key(&entity.name) {
                    entities.insert(entity.name.clone(), HashMap::new());
                }

                assert!(
                    entities.get_mut(&entity.name).unwrap()
                    .insert(fmt_lang(lang_set.language.clone()), def)
                    .is_none()
                )


                // TODO! Need a way of passing entities
            }
        }

        (
            new_intents.into_iter().map(
                |(intent, utts)| {
                let action: Arc<Mutex<dyn Action + Send>> = Arc::new(
                    Mutex::new(
                        VapAction::new(
                            fmt_name(&intent),
                            "TODO!Figure ip".to_string(),
                            self.out.clone()
                        )
                    )
                );
                (intent, utts, action)
            }).collect(),

            entities.into_iter().collect()
        )
    }
}

#[async_trait(?Send)]
impl SkillLoader for VapLoader {
    fn load_skills(&mut self, _langs: &Vec<LanguageIdentifier>) -> Result<()> {
        Ok(())
    }
    
    async fn run_loader(&mut self) -> Result<()> {
        let (stream, reg) = self.stream_reg.take().unwrap();
        tokio::select!(
            _= reg.run() => {},
            _= self.on_msg(stream) => {}
        );

        Ok(())
    }
}

struct VapAction {
    name: String,
    ip: String,
    next_request_id: u64,
    shared_out: Arc<Mutex<SkillRegisterOut>>,
}

impl VapAction {
    pub fn new(name: String, ip: String, shared_out: Arc<Mutex<SkillRegisterOut>>) -> Self {
        VapAction {name, ip, next_request_id: 0, shared_out}
    }
}

#[async_trait(?Send)]
impl Action for VapAction {
    async fn call(&mut self, context: &ActionContext) -> Result<ActionAnswer> {
        // TODO! Also map events!!!

        let slots = context.data.as_intent()
            .unwrap()
            .slots.iter()
            .map(|(n,v)| RequestSlot {
                name: n.clone(),
                value: Some(v.clone())
            }).collect();

        let (capabilities,sender) = self.shared_out.lock_it().activate_skill(self.ip.clone(), MsgSkillRequest {
            request_id: self.next_request_id,
            client: ClientData {
                system_id: context.satellite.as_ref().map(|s|s.uuid.clone()).expect("No satellite"),
                capabilities: vec![], // TODO! Figure out capabilities
            },
            request: RequestData {
                type_:  RequestDataKind::Intent,
                intent: context.data.as_intent().unwrap().name.clone(),
                locale: context.locale.clone(),
                slots
            }
        }).await?;

        sender.send(RequestResponse {
            code: 205,
        }).unwrap();

        let data = if capabilities[0].name == "voice" {
            capabilities[0].cap_data["text"].clone()
        } else {
            "NO VOICE CAPABILITY IN RESPONSE".to_string()
        };
        ActionAnswer::send_text(data, true)
    }

    fn get_name(&self) -> String {
        self.name.clone()
    }
}