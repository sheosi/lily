use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::config::Config;
use crate::exts::LockIt;
use crate::signals::order::{server_actions::SendData, dev_mgmt::{CAPS_MANAGER, SessionManager}};
use crate::tts::{Gender, Tts, TtsFactory, VoiceDescr};

use anyhow::Result;
use lily_common::{audio::Audio, communication::*, vars::DEFAULT_SAMPLES_PER_SECOND};
use log::{error, info, warn};
use rmp_serde::{decode, encode};
use rumqttc::{AsyncClient, Event, EventLoop, Packet, QoS};
use tokio::{try_join, sync::mpsc};
use unic_langid::LanguageIdentifier;

thread_local! {
    pub static MSG_OUTPUT: RefCell<Option<MqttInterfaceOutput>> = RefCell::new(None);
}

pub struct MqttInterface {
    common_out: mpsc::Receiver<(SendData, String)>
}

impl MqttInterface {
    pub fn new() -> Result<Self> {
        let (sender, common_out) = mpsc::channel(100);
        let output = MqttInterfaceOutput::create(sender)?;
        MSG_OUTPUT.with(|a|a.replace(Some(output)));

        Ok(Self {common_out})
    }


    pub async fn interface_loop (
        &mut self,
        config: &Config,
        curr_langs: &Vec<LanguageIdentifier>,
        def_lang: Option<&LanguageIdentifier>,
        sessions: Arc<Mutex<SessionManager>>,
        channel_nlu: mpsc::Sender<MsgRequest>,
        channel_event: mpsc::Sender<MsgEvent>,
    ) -> Result<()> {
            
        let mqtt_conf = ConnectionConfResolved::from(
            config.mqtt.clone(),
            || "lily-server".into()
        );
        let (client_raw, eloop) = make_mqtt_conn(&mqtt_conf, None)?;
        client_raw.subscribe("lily/new_satellite", QoS::AtMostOnce).await?;
        client_raw.subscribe("lily/nlu_process", QoS::AtMostOnce).await?;
        client_raw.subscribe("lily/event", QoS::AtMostOnce).await?;
        client_raw.subscribe("lily/disconnected", QoS::ExactlyOnce).await?;
        let client = Arc::new(Mutex::new(client_raw));

        let voice_prefs: VoiceDescr = VoiceDescr {
            gender:if config.tts.prefer_male{Gender::Male}else{Gender::Female}
        };

        let mut tts_set = HashMap::new();
        for lang in curr_langs {
            let tts = TtsFactory::load_with_prefs(lang, config.tts.prefer_online, config.tts.ibm.clone(), &voice_prefs)?;
            info!("Using tts {}", tts.get_info());
            tts_set.insert(lang, tts);
        }
       
        async fn handle_in(
            mut eloop: EventLoop,
            client: Arc<Mutex<AsyncClient>>,
            config: &Config,
            channel_nlu: mpsc::Sender<MsgRequest>,
            channel_event: mpsc::Sender<MsgEvent>,
        ) -> Result<()> {
            loop {
                let notification = eloop.poll().await?;
                //println!("Notification = {:?}", notification);
                match notification {
                    Event::Incoming(Packet::Publish(pub_msg)) => {
                        match pub_msg.topic.as_str() {
                            "lily/new_satellite" => {
                                info!("New satellite incoming");
                                let input :MsgNewSatellite = decode::from_read(std::io::Cursor::new(pub_msg.payload))?;
                                let uuid2 = &input.uuid;
                                let caps = input.caps;
                                CAPS_MANAGER.with(|c| c.borrow_mut().add_client(&uuid2, caps));
                                let output = encode::to_vec(&MsgWelcome{conf:config.to_client_conf(), satellite: input.uuid})?;
                                client.lock_it().publish("lily/satellite_welcome", QoS::AtMostOnce, false, output).await?
                            }
                            "lily/nlu_process" => {
                                let msg_nlu: MsgRequest = decode::from_read(std::io::Cursor::new(pub_msg.payload))?;
                                channel_nlu.send(msg_nlu).await?;
                            }
                            "lily/event" => {
                                let msg: MsgEvent = decode::from_read(std::io::Cursor::new(pub_msg.payload))?;
                                channel_event.send(msg).await?;
                            }
                            "lily/disconnected" => {
                                let msg: MsgGoodbye = decode::from_read(std::io::Cursor::new(pub_msg.payload))?;
                                if let Err(e) = CAPS_MANAGER.with(|c|c.borrow_mut().disconnected(&msg.satellite)) {
                                    warn!("{}",&e.to_string())
                                }
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }


        async fn handle_out(
            common_out: &mut mpsc::Receiver<(SendData, String)>,
            tts_set: &mut HashMap<&LanguageIdentifier, Box<dyn Tts>>,
            def_lang: Option<&LanguageIdentifier>,
            sessions: Arc<Mutex<SessionManager>>,
            client: Arc<Mutex<AsyncClient>>
        ) -> Result<()> {
            loop {
                let (msg_data, uuid_str) = common_out.recv().await.expect("Out channel broken");
                process_out(msg_data, uuid_str, tts_set, def_lang.clone(), &sessions, &client).await?;
            }
        }
            
        async fn process_out(
            msg_data: SendData,
            uuid_str: String,
            tts_set: &mut HashMap<&LanguageIdentifier, Box<dyn Tts>>,
            def_lang: Option<&LanguageIdentifier>,
            sessions: &Arc<Mutex<SessionManager>>,
            client: &Arc<Mutex<AsyncClient>>
        ) -> Result<()> {
            let audio_data = match msg_data {
                SendData::Audio(audio) => {
                    audio
                }
                SendData::String((str, lang)) => {
                    async fn synth_text(tts: &mut Box<dyn Tts>, input: &str) -> Audio {
                        match tts.synth_text(input).await {
                            Ok(a) => a,
                            Err(e) => {
                                error!("Error while synthing voice: {}", e);
                                Audio::new_empty(DEFAULT_SAMPLES_PER_SECOND)
                            }
                        }
                    }

                    match tts_set.get_mut(&lang) {
                        Some(tts) => {
                            synth_text(tts, &str).await
                        }
                        None => {
                            warn!("Received answer for language {:?} not in the config or that has no TTS, using default", lang);
                            let def = def_lang.expect("There's no language assigned, need one at least");
                            match tts_set.get_mut(def) {
                                Some(tts) => {
                                    synth_text(tts, &str).await
                                }
                                None => {
                                    warn!("Default has no tts either, sending empty audio");
                                    Audio::new_empty(DEFAULT_SAMPLES_PER_SECOND)
                                }
                            } 
                        }
                    }
                }
            };

            if let Err(e) = sessions.lock().expect("POISON_MSG").end_session(&uuid_str) {
                warn!("{}",e);
            }
            let msg_pack = encode::to_vec(&MsgAnswer{audio: Some(audio_data.into_encoded()?), text: None})?;
            client.lock_it().publish(&format!("lily/{}/say_msg", uuid_str), QoS::AtMostOnce, false, msg_pack).await?;
            Ok(())
        }

        let i = handle_in(eloop, client.clone(), config, channel_nlu, channel_event);
        let o = handle_out(&mut self.common_out, &mut tts_set, def_lang, sessions, client);
        try_join!(i, o)?;
                
        Ok(())
    }
}

pub struct MqttInterfaceOutput {
    client: mpsc::Sender<(SendData, String)>
}

impl MqttInterfaceOutput {
    fn create(client: mpsc::Sender<(SendData, String)>) -> Result<Self> {
        Ok(Self{client})
    }

    pub fn answer(&mut self, input: String, lang: &LanguageIdentifier, to: String) -> Result<()> {
        self.client.try_send((SendData::String((input, lang.to_owned())), to)).unwrap();
        Ok(())
    }

    pub fn send_audio(&mut self, audio: Audio, to: String) -> Result<()> {
        self.client.try_send((SendData::Audio(audio), to)).unwrap();
        Ok(())
    }
}