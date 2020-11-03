use std::collections::HashMap;
use std::mem::replace;
use std::sync::{Arc, Mutex};
use std::ops::DerefMut;

use crate::config::Config;
use crate::nlu::{NluManager, NluManagerConf, NluManagerStatic};
use crate::signals::{SignalEventShared, SignalOrder};
use crate::stt::{DecodeState, IbmSttData, SttFactory};
use crate::tts::{Gender, TtsFactory, VoiceDescr};
use crate::vars::DEFAULT_SAMPLES_PER_SECOND;

use anyhow::Result;
use lazy_static::lazy_static;
use lily_common::audio::AudioRaw;
use lily_common::communication::*;
use log::info;
use pyo3::{types::PyDict, Py};
use rmp_serde::{decode, encode};
use rumqttc::{Event, MqttOptions, Client, Packet, QoS};
use serde::Deserialize;
use uuid::Uuid;
use unic_langid::LanguageIdentifier;
use url::Url;

lazy_static!{
    pub static ref MSG_OUTPUT: Mutex<Option<MqttInterfaceOutput>> = Mutex::new(None);
    pub static ref LAST_SITE: Mutex<Option<Uuid>> = Mutex::new(None);
}

#[derive(Clone, Debug, Deserialize)]
pub struct MqttConfig {
    #[serde(default = "def_broker")]
    broker: String
}

fn def_broker() -> String {
    "127.0.0.1".to_owned()
}

impl Default for MqttConfig {
    fn default() -> Self {
        Self {
            broker: def_broker()
        }
    }
}


struct SiteManager {
    map: HashMap<String,Vec<Uuid>>
}

impl SiteManager {
    // TODO: What to do on name collisions?
    fn new() -> Self {
        Self{
            map: HashMap::new()
        }
    }

    fn new_site(&mut self, name: String) -> Uuid {
        let uuid = Uuid::new_v4();
        if !self.map.contains_key(&name) {
            self.map.insert(name, vec![uuid]);
        }
        else {
           self.map.get_mut(&name).as_deref_mut().unwrap().push(uuid);
        }

        uuid
    }
}

pub struct MqttInterface {
    common_out: Arc<Mutex<Vec<(String, String)>>>,
    curr_lang: LanguageIdentifier,
    ibm_data: Option<IbmSttData>
}

impl MqttInterface {
    pub fn new(curr_lang: &LanguageIdentifier, config: &Config) -> Self {
        let common_out = Arc::new(Mutex::new(Vec::new()));
        let ibm_data = config.extract_ibm_stt_data();
        let output = MqttInterfaceOutput::create(common_out.clone());
        MSG_OUTPUT.lock().unwrap().replace(output.clone());
        Self {
            common_out,
            curr_lang: curr_lang.to_owned(),
            ibm_data
        }
    }

    pub async fn interface_loop<M: NluManager + NluManagerConf + NluManagerStatic> (&mut self, config: &Config, signal_event: SignalEventShared, base_context: &Py<PyDict>, order: &mut SignalOrder<M>) -> Result<()> {
        let mut sites = SiteManager::new();
        let mqtt_conf = config.mqtt_conf.clone().unwrap_or(MqttConfig::default());
        let url = Url::parse(
            &format!("http://{}", mqtt_conf.broker) // Won't work without protocol
        ).unwrap();
        let host = url.host_str().unwrap();
        let port: u16 = url.port().unwrap_or(1883);
        let mut mqttoptions = MqttOptions::new("lily-server", host, port);
        // TODO: Set username and passwd
        mqttoptions.set_keep_alive(5);
    
        let (mut client, mut connection) = Client::new(mqttoptions, 10);
        client.subscribe("lily/nlu_process", QoS::AtMostOnce).unwrap();

        let dummy_sample = AudioRaw::new_empty(DEFAULT_SAMPLES_PER_SECOND);
        let mut stt = SttFactory::load(&self.curr_lang, &dummy_sample,  config.prefer_online_stt, self.ibm_data.clone()).await?;
        info!("Using stt {}", stt.get_info());

        const VOICE_PREFS: VoiceDescr = VoiceDescr {gender: Gender::Female};
        let ibm_tts_gateway_key = config.extract_ibm_tts_data();

        let mut tts = TtsFactory::load_with_prefs(&self.curr_lang, config.prefer_online_tts, ibm_tts_gateway_key.clone(), &VOICE_PREFS)?;

        for notification in connection.iter() {
            
            println!("Notification = {:?}", notification);
            match notification.unwrap() {
                Event::Incoming(Packet::Publish(pub_msg)) => {
                    match pub_msg.topic.as_str() {
                        "lily/new_satellite" => {
                            let input :MsgNewSatellite = decode::from_read(std::io::Cursor::new(pub_msg.payload))?;
                            let output = encode::to_vec(&MsgWelcome{conf:config.to_client_conf(), uuid: sites.new_site(input.name.clone()), name: input.name})?;
                            client.publish("lily/satellite_welcome", QoS::AtMostOnce, false, output)?
                        }
                        "lily/nlu_process" => {
                            let msg_nlu: MsgNluVoice = decode::from_read(std::io::Cursor::new(pub_msg.payload))?;
                            let as_raw = AudioRaw::from_ogg_opus(msg_nlu.audio)?;
                            match stt.decode(&as_raw.buffer).await? {
                                DecodeState::Finished(decode_res) => {
                                    order.received_order(decode_res, signal_event.clone(), base_context).await?;
                                }
                                
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }

            {   
                let msg_vec = replace(self.common_out.lock().unwrap().deref_mut(), Vec::new());
                for (msg, uuid_str) in msg_vec {
                    let synth_audio = tts.synth_text(&msg).await?;
                    let msg_pack = encode::to_vec(&MsgAnswerVoice{data: synth_audio.into_encoded()?}).unwrap(); 
                    client.publish(format!("lily/{}/say_msg", uuid_str), QoS::AtMostOnce, false, msg_pack).unwrap();
                }
            }
        }

        Ok(())
    }
}

#[derive(Clone)]
pub struct MqttInterfaceOutput {
    common_out: Arc<Mutex<Vec<(String, String)>>>
}

impl MqttInterfaceOutput {
    fn create(common_out: Arc<Mutex<Vec<(String, String)>>>) -> Self {
        Self{common_out}
    }

    pub fn answer(&mut self, input: &str, to: String) -> Result<()> {
        self.common_out.lock().unwrap().push((input.to_owned(), to.to_owned()));
        Ok(())
    }
}

