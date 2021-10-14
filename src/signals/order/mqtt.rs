use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::config::{Config};
use crate::exts::LockIt;
use crate::signals::order::{server_actions::SendData, dev_mgmt::{CAPS_MANAGER, SessionManager}};
use crate::tts::{Gender, Tts, TtsData, TtsFactory, VoiceDescr};

use anyhow::Result;
use bytes::Bytes;
use lily_common::{audio::Audio, communication::*, vars::DEFAULT_SAMPLES_PER_SECOND};
use log::{error, info, warn};
use rmp_serde::{decode, encode};
use rumqttc::{AsyncClient, QoS};
use tokio::{sync::mpsc};
use unic_langid::LanguageIdentifier;

thread_local! {
    pub static MSG_OUTPUT: RefCell<Option<MqttInterfaceOutput>> = RefCell::new(None);
}

pub struct MqttInterfaceIn {
}

impl MqttInterfaceIn {
    pub fn new() -> Self {
        Self{}
    }

    pub async fn handle_new_sattelite(&mut self, payload: &Bytes, config: &Config, client: &Arc<Mutex<AsyncClient>>) -> Result<()> {
        info!("New satellite incoming");
        let input :MsgNewSatellite = decode::from_read(std::io::Cursor::new(payload))?;
        let uuid2 = &input.uuid;
        let caps = input.caps;
        CAPS_MANAGER.with(|c| c.borrow_mut().add_client(&uuid2, caps));
        let output = encode::to_vec(&MsgWelcome{conf:config.to_client_conf(), satellite: input.uuid})?;
        Ok(client.lock_it().publish("lily/satellite_welcome", QoS::AtMostOnce, false, output).await?)
    }

    pub async fn handle_nlu_process(&mut self, payload: &Bytes, channel_nlu: &mpsc::Sender<MsgRequest>) -> Result<()> {
        let msg_nlu: MsgRequest = decode::from_read(std::io::Cursor::new(payload))?;
        Ok(channel_nlu.send(msg_nlu).await?)
    }

    pub async fn handle_event(&mut self, payload: &Bytes, channel_event: &mpsc::Sender<MsgEvent>) -> Result<()> {
        let msg: MsgEvent = decode::from_read(std::io::Cursor::new(payload))?;
        Ok(channel_event.send(msg).await?)
    }

    pub async fn handle_disconnected(&mut self, payload: &Bytes) -> Result<()> {
        let msg: MsgGoodbye = decode::from_read(std::io::Cursor::new(payload))?;

        // This error has nothing to do with connection, shouldn't break it
        if let Err(e) = CAPS_MANAGER.with(|c|c.borrow_mut().disconnected(&msg.satellite)) {
            warn!("{}",&e.to_string())
        }

        Ok(())
    }

    pub async fn subscribe(client: &Arc<Mutex<AsyncClient>>) -> Result<()> {
        let client_raw = client.lock_it();
        client_raw.subscribe("lily/new_satellite", QoS::AtMostOnce).await?;
        client_raw.subscribe("lily/nlu_process", QoS::AtMostOnce).await?;
        client_raw.subscribe("lily/event", QoS::AtMostOnce).await?;
        client_raw.subscribe("lily/disconnected", QoS::ExactlyOnce).await?;

        Ok(())
    }

}
pub struct MqttInterfaceOut {
    common_out: mpsc::Receiver<(SendData, String)>
}

impl MqttInterfaceOut {
    pub fn new() -> Result<Self> {
        let (sender, common_out) = mpsc::channel(100);
        let output = MqttInterfaceOutput::create(sender)?;
        MSG_OUTPUT.with(|a|a.replace(Some(output)));

        Ok(Self {common_out})
    }

    pub async fn handle_out(
        &mut self,
        curr_langs: &Vec<LanguageIdentifier>,
        conf_tts: &TtsData,
        def_lang: Option<&LanguageIdentifier>,
        sessions: Arc<Mutex<SessionManager>>,
        client: &Arc<Mutex<AsyncClient>>,

    ) -> Result<()> {
        let voice_prefs: VoiceDescr = VoiceDescr {
            gender:if conf_tts.prefer_male{Gender::Male}else{Gender::Female}
        };

        let mut tts_set = HashMap::new();
        for lang in curr_langs {
            let tts = TtsFactory::load_with_prefs(lang, conf_tts.prefer_online, conf_tts.ibm.clone(), &voice_prefs)?;
            info!("Using tts {}", tts.get_info());
            tts_set.insert(lang, tts);
        }

        loop {
            let (msg_data, uuid_str) = self.common_out.recv().await.expect("Out channel broken");
            Self::process_out(msg_data, uuid_str, &mut tts_set, &def_lang, &sessions, client).await?;
        }
    }
        
    async fn process_out(
        msg_data: SendData,
        uuid_str: String,
        tts_set: &mut HashMap<&LanguageIdentifier, Box<dyn Tts>>,
        def_lang: &Option<&LanguageIdentifier>,
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
                        let def = def_lang.clone().expect("There's no language assigned, need one at least");
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