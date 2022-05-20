// Standard library
use std::collections::{hash_map, HashMap};
use std::str::FromStr;
use std::sync::{Arc, Mutex};

// This crate
use crate::actions::{Action, ActionAnswer, ActionContext};
use crate::exts::LockIt;
use crate::nlu::{EntityData, EntityDef, IntentData, OrderKind, SlotData};
use crate::signals::collections::Hook;
use crate::skills::{register_skill, SkillLoader};

// Other crates
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use maplit::hashmap;
use rmp_serde::to_vec_named;
use unic_langid::{subtags, LanguageIdentifier};
use vap_common_skill::structures::msg_query_response::QueryDataCapability;
use vap_common_skill::structures::msg_register_intents::NluData;
use vap_common_skill::structures::msg_skill_request::{
    ClientData, RequestData, RequestDataKind, RequestSlot,
};
use vap_common_skill::structures::{
    msg_notification_response, msg_query_response, AssociativeMap, Language, MsgConnectResponse,
    MsgNotificationResponse, MsgQueryResponse, MsgRegisterIntentsResponse, MsgSkillRequest,
};
use vap_skill_register::{
    RequestResponse, Response, ResponseType, SkillRegister, SkillRegisterMessage, SkillRegisterOut,
    SkillRegisterStream, SYSTEM_SELF_ID,
};

pub struct VapLoader {
    out: Arc<Mutex<SkillRegisterOut>>,
    stream_reg: Option<(SkillRegisterStream, SkillRegister)>,
    langs: Vec<Language>,

    // Dicts containing the system capabilities
    notifies: HashMap<String, Box<dyn CanBeNotified>>,
    queries: HashMap<String, Box<dyn CanBeQueried>>,
}

impl VapLoader {
    pub fn new(port: u16, langs: Vec<LanguageIdentifier>) -> Self {
        let (reg, stream, out) = SkillRegister::new(port).unwrap();
        let langs = langs.into_iter().map(|l| l.into()).collect();

        VapLoader {
            out: Arc::new(Mutex::new(out)),
            stream_reg: Some((stream, reg)),
            langs,

            notifies: HashMap::new(),
            queries: HashMap::new(),
        }
    }

    pub fn register_notify(&mut self, name: String, caller: Box<dyn CanBeNotified>) -> Result<()> {
        if let hash_map::Entry::Vacant(e) = self.notifies.entry(name) {
            e.insert(caller);
            Ok(())
        } else {
            Err(anyhow!("Notify already exists"))
        }
    }

    pub fn register_query(&mut self, name: String, caller: Box<dyn CanBeQueried>) -> Result<()> {
        if let hash_map::Entry::Vacant(e) = self.queries.entry(name) {
            e.insert(caller);
            Ok(())
        } else {
            Err(anyhow!("Query already exists"))
        }
    }

    async fn on_msg(
        &mut self,
        mut stream: SkillRegisterStream,
    ) -> Result<(), vap_skill_register::Error> {
        loop {
            let (msg, responder) = stream.recv().await?;

            let response = match msg {
                SkillRegisterMessage::Connect(_) => {
                    // SkillRegister already makes sure that the skill name is
                    // not one known and the vap version is compatible, there's
                    // nothing more to do
                    Response {
                        status: ResponseType::Valid,
                        payload: to_vec_named(&MsgConnectResponse {
                            langs: self.langs.clone(),
                            unique_authentication_token: Some("".into()), // TODO: Finish security
                        })
                        .unwrap(),
                    }
                }

                SkillRegisterMessage::RegisterIntents(msg) => {
                    let (actions, entities) = self.transform(msg.nlu_data);
                    match register_skill(&msg.skill_id, actions, vec![], vec![], entities) {
                        Ok(()) => Response {
                            status: ResponseType::Valid,
                            payload: to_vec_named(&MsgRegisterIntentsResponse {}).unwrap(),
                        },

                        Err(_) => {
                            Response {
                                status: ResponseType::RequestEntityIncomplete,
                                // TODO! Add why
                                payload: vec![],
                            }
                        }
                    }
                }

                SkillRegisterMessage::Close(_) => {
                    // TODO! We should unregister the skill from the NLU
                    Response {
                        status: ResponseType::Valid,
                        payload: Vec::new(),
                    }
                }

                SkillRegisterMessage::Notification(msg) => {
                    let data = msg
                        .data
                        .into_iter()
                        .map(|n| {
                            // TODO! Should we send an answer per capability?
                            let code = if n.client_id == SYSTEM_SELF_ID {
                                n.capabilities
                                    .into_iter()
                                    .map(|c| {
                                        if self.notifies.contains_key(&c.name) {
                                            match self
                                                .notifies
                                                .get_mut(&c.name)
                                                .unwrap()
                                                .notify(c.cap_data)
                                            {
                                                NotificationResult::Valid => 200,
                                            }
                                        } else {
                                            404
                                        }
                                    })
                                    .max()
                                    .unwrap_or(200)
                            }
                            // else if ... // TODO! Notification need to be sent to clients
                            else {
                                404
                            };
                            msg_notification_response::Data::StandAlone {
                                client_id: n.client_id,
                                code,
                            }
                        })
                        .collect::<Vec<_>>();

                    Response {
                        status: ResponseType::Valid,
                        payload: to_vec_named(&MsgNotificationResponse { data }).unwrap(),
                    }
                }

                SkillRegisterMessage::Query(msg) => {
                    let data = msg.data.into_iter().map(
                        |q| {
                            let client_id = q.client_id;
                            let capabilities = if client_id == SYSTEM_SELF_ID {
                                q.capabilities.into_iter().map(|c|{
                                    if let hash_map::Entry::Occupied(mut e) = self.queries.entry(c.name.clone()) {
                                        let (code, data) = e.get_mut().query(c.cap_data).unwrap();

                                        QueryDataCapability {name: c.name, code, data}
                                    }
                                    else {
                                        QueryDataCapability {
                                            name: c.name.clone(),
                                            code: 404,
                                            data: hashmap!{"object".into() => c.name.into()}
                                        }
                                    }
                                }).collect()
                            }
                            // else if  ... // TODO! Queries need to be sent to clients
                            else {
                                q.capabilities.into_iter().map(|c|{
                                    QueryDataCapability { name: c.name, code: 404, data: hashmap!{"object".into() => client_id.clone().into()}}
                                }).collect()
                            };

                            msg_query_response::QueryData {client_id, capabilities}
                        }
                    ).collect();

                    Response {
                        status: ResponseType::Valid,
                        payload: to_vec_named(&MsgQueryResponse { data }).unwrap(),
                    }
                }
            };

            responder
                .send(response)
                .map_err(|_| vap_skill_register::Error::ClosedChannel)?;
        }
    }

    fn transform(
        &self,
        nlu_data: Vec<NluData>,
    ) -> (
        Vec<(
            String,
            HashMap<LanguageIdentifier, IntentData>,
            Arc<Mutex<dyn Action + Send>>,
        )>,
        Vec<(String, HashMap<LanguageIdentifier, EntityDef>)>,
    ) {
        let mut new_intents: HashMap<String, HashMap<LanguageIdentifier, IntentData>> =
            HashMap::new();
        let mut entities: HashMap<String, HashMap<LanguageIdentifier, EntityDef>> = HashMap::new();

        fn fmt_name(name: &str) -> String {
            format!("vap_action_{}", name)
        }

        fn fmt_lang(lang: Language) -> LanguageIdentifier {
            LanguageIdentifier::from_parts(
                subtags::Language::from_str(&lang.language).unwrap(),
                Option::None,
                lang.country
                    .and_then(|r| subtags::Region::from_str(&r).ok()),
                &lang
                    .extra
                    .and_then(|e| subtags::Variant::from_str(&e).ok())
                    .map(|v| vec![v])
                    .unwrap_or_else(Vec::new),
            )
        }

        for lang_set in nlu_data.into_iter() {
            for intent in lang_set.intents {
                let internal_intent = IntentData {
                    slots: intent
                        .slots
                        .into_iter()
                        .map(|s| {
                            (
                                s.name.clone(),
                                SlotData {
                                    slot_type: OrderKind::Ref(s.entity),
                                    required: false,
                                    prompt: None,
                                    reprompt: None,
                                },
                            )
                        })
                        .collect(),
                    utts: intent.utterances.into_iter().map(|d| d.text).collect(), // TODO! Utterances might need some conversion of slot format
                    hook: Hook::Action(fmt_name(&intent.name)),
                };

                if let hash_map::Entry::Vacant(e) = new_intents.entry(intent.name.clone()) {
                    e.insert(HashMap::new());
                }

                assert!(new_intents
                    .get_mut(&intent.name)
                    .unwrap()
                    .insert(fmt_lang(lang_set.language.clone()), internal_intent)
                    .is_none());
            }

            for entity in lang_set.entities {
                let def = EntityDef {
                    data: entity
                        .data
                        .into_iter()
                        .map(|d| EntityData {
                            value: d.value,
                            synonyms: d.synonyms,
                        })
                        .collect(),
                    automatically_extensible: !entity.strict,
                };

                if !entities.contains_key(&entity.name) {
                    entities.insert(entity.name.clone(), HashMap::new());
                }

                assert!(entities
                    .get_mut(&entity.name)
                    .unwrap()
                    .insert(fmt_lang(lang_set.language.clone()), def)
                    .is_none())

                // TODO! Need a way of passing entities
            }
        }

        (
            new_intents
                .into_iter()
                .map(|(intent, utts)| {
                    let action: Arc<Mutex<dyn Action + Send>> =
                        Arc::new(Mutex::new(VapAction::new(
                            fmt_name(&intent),
                            "TODO!Figure ip".to_string(),
                            self.out.clone(),
                        )));
                    (intent, utts, action)
                })
                .collect(),
            entities.into_iter().collect(),
        )
    }
}

#[async_trait(?Send)]
impl SkillLoader for VapLoader {
    fn load_skills(&mut self, _langs: &[LanguageIdentifier]) -> Result<()> {
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
        VapAction {
            name,
            ip,
            next_request_id: 0,
            shared_out,
        }
    }
}

#[async_trait(?Send)]
impl Action for VapAction {
    async fn call(&mut self, context: &ActionContext) -> Result<ActionAnswer> {
        // TODO! Also map events!!!

        let slots = context
            .data
            .as_intent()
            .unwrap()
            .slots
            .iter()
            .map(|(n, v)| RequestSlot {
                name: n.clone(),
                value: Some(v.clone()),
            })
            .collect();

        let (capabilities, sender) = self
            .shared_out
            .lock_it()
            .activate_skill(
                self.ip.clone(),
                MsgSkillRequest {
                    request_id: self.next_request_id,
                    client: ClientData {
                        system_id: context
                            .satellite
                            .as_ref()
                            .map(|s| s.uuid.clone())
                            .expect("No satellite"),
                        capabilities: vec![], // TODO! Figure out capabilities
                    },
                    request: RequestData {
                        type_: RequestDataKind::Intent,
                        intent: context.data.as_intent().unwrap().name.clone(),
                        locale: context.locale.clone(),
                        slots,
                    },
                },
            )
            .await?;

        sender.send(RequestResponse { code: 205 }).unwrap();

        let data = if capabilities[0].name == "voice" {
            capabilities[0].cap_data[&"text".into()].clone().to_string()
        } else {
            "NO VOICE CAPABILITY IN RESPONSE".to_string()
        };
        ActionAnswer::send_text(data, true)
    }

    fn get_name(&self) -> String {
        self.name.clone()
    }
}

// Capabilities /**************************************************************/
pub trait CanBeQueried {
    fn query(&mut self, caps: AssociativeMap) -> Result<(u16, AssociativeMap)>;
}

pub enum NotificationResult {
    Valid,
}

pub trait CanBeNotified {
    fn notify(&mut self, caps: AssociativeMap) -> NotificationResult;
}
