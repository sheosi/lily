use std::cell::RefCell;
use std::collections::HashMap;
use std::mem::replace;
use std::sync::{Arc, Mutex};
use std::ops::DerefMut;

use crate::config::Config;
use crate::nlu::{NluManager, NluManagerConf, NluManagerStatic};
use crate::signals::{SignalEventShared, SignalOrder};
use crate::stt::{DecodeState, SttFactory};
use crate::tts::{Gender, TtsFactory, VoiceDescr};
use crate::vars::DEFAULT_SAMPLES_PER_SECOND;

use anyhow::Result;
use lily_common::audio::{Audio, AudioRaw};
use lily_common::communication::*;
use lily_common::extensions::MakeSendable;
use lily_common::other::ConnectionConf;
use log::info;
use pyo3::{types::PyDict, Py};
use rmp_serde::{decode, encode};
use rumqttc::{Event, Packet, QoS};
use uuid::Uuid;
use unic_langid::LanguageIdentifier;

thread_local!{
    pub static MSG_OUTPUT: RefCell<Option<MqttInterfaceOutput>> = RefCell::new(None);
    pub static LAST_SITE: RefCell<Option<Uuid>> = RefCell::new(None);
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
    common_out: Arc<Mutex<Vec<(SendData, String)>>>,
    curr_lang: LanguageIdentifier
}

impl MqttInterface {
    pub fn new(curr_lang: &LanguageIdentifier) -> Result<Self> {
        let common_out = Arc::new(Mutex::new(vec![]));
        let output = MqttInterfaceOutput::create(common_out.clone())?;
        MSG_OUTPUT.with(|a|a.replace(Some(output)));

        Ok(Self {
            common_out,
            curr_lang: curr_lang.to_owned(),
        })
    }

    pub async fn interface_loop<M: NluManager + NluManagerConf + NluManagerStatic> (&mut self, config: &Config, signal_event: SignalEventShared, base_context: &Py<PyDict>, order: &mut SignalOrder<M>) -> Result<()> {
        let ibm_data = config.extract_ibm_stt_data();
        let mqtt_conf = &config.mqtt.clone().unwrap_or(ConnectionConf::default());
        let (client, mut eloop) = make_mqtt_conn(mqtt_conf);
        let mut sites = SiteManager::new();

        client.subscribe("lily/new_satellite", QoS::AtMostOnce).await?;
        client.subscribe("lily/nlu_process", QoS::AtMostOnce).await?;

        let dummy_sample = AudioRaw::new_empty(DEFAULT_SAMPLES_PER_SECOND);
        let mut stt = SttFactory::load(&self.curr_lang, &dummy_sample,  config.prefer_online_stt, ibm_data).await?;
        info!("Using stt {}", stt.get_info());

        const VOICE_PREFS: VoiceDescr = VoiceDescr {gender: Gender::Female};
        let ibm_tts_gateway_key = config.extract_ibm_tts_data();

        let mut tts = TtsFactory::load_with_prefs(&self.curr_lang, config.prefer_online_tts, ibm_tts_gateway_key.clone(), &VOICE_PREFS)?;
        info!("Using tts {}", tts.get_info());

        loop {
            let notification = eloop.poll().await?;
            println!("Notification = {:?}", notification);
            match notification {
                Event::Incoming(Packet::Publish(pub_msg)) => {
                    match pub_msg.topic.as_str() {
                        "lily/new_satellite" => {
                            info!("New satellite incoming");
                            let input :MsgNewSatellite = decode::from_read(std::io::Cursor::new(pub_msg.payload))?;
                            let output = encode::to_vec(&MsgWelcome{conf:config.to_client_conf(), uuid: sites.new_site(input.name.clone()), name: input.name})?;
                            client.publish("lily/satellite_welcome", QoS::AtMostOnce, false, output).await?
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
                for (msg_data, uuid_str) in msg_vec {

                    let audio_data = match msg_data {
                        SendData::Audio(audio) => {
                            audio
                        }
                        SendData::String(str) => {
                            tts.synth_text(&str).await?
                        }
                    };
                    let msg_pack = encode::to_vec(&MsgAnswerVoice{data: audio_data.into_encoded()?}).unwrap();
                    client.publish(format!("lily/{}/say_msg", uuid_str), QoS::AtMostOnce, false, msg_pack).await?;
                }
            }
        }
    }
}

enum SendData {
    String(String),
    Audio(Audio)
}
pub struct MqttInterfaceOutput {
    client: Arc<Mutex<Vec<(SendData, String)>>>
}

impl MqttInterfaceOutput {
    fn create(client: Arc<Mutex<Vec<(SendData, String)>>>) -> Result<Self> {
        Ok(Self{client})
    }

    pub fn answer(&mut self, input: &str, to: String) -> Result<()> {
        self.client.lock().sendable()?.push((SendData::String(input.into()), to));
        Ok(())
    }

    pub fn send_audio(&mut self, audio: Audio, to: String) -> Result<()> {
        self.client.lock().sendable()?.push((SendData::Audio(audio), to));
        Ok(())
    }
}

