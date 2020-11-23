use std::cell::RefCell;
use std::mem::replace;
use std::sync::{Arc, Mutex};
use std::ops::DerefMut;

use crate::config::Config;
use crate::nlu::{NluManager, NluManagerConf, NluManagerStatic};
use crate::signals::{SignalEventShared, SignalOrder};
use crate::stt::SttFactory;
use crate::tts::{Gender, TtsFactory, VoiceDescr};

use anyhow::Result;
use lily_common::audio::{Audio, decode_ogg_opus};
use lily_common::communication::*;
use lily_common::extensions::MakeSendable;
use log::{error, info};
use pyo3::{types::PyDict, Py};
use rmp_serde::{decode, encode};
use rumqttc::{Event, Packet, QoS};
use unic_langid::LanguageIdentifier;

thread_local!{
    pub static MSG_OUTPUT: RefCell<Option<MqttInterfaceOutput>> = RefCell::new(None);
    pub static LAST_SITE: RefCell<Option<String>> = RefCell::new(None);
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
        let ibm_data = config.stt.ibm.clone();
        let mqtt_conf = ConnectionConfResolved::from(
            config.mqtt.clone(),
            || "lily-server".into()
        );
        let (client, mut eloop) = make_mqtt_conn(&mqtt_conf);
        

        client.subscribe("lily/new_satellite", QoS::AtMostOnce).await?;
        client.subscribe("lily/nlu_process", QoS::AtMostOnce).await?;
        client.subscribe("lily/event", QoS::AtMostOnce).await?;

        let mut stt = SttFactory::load(&self.curr_lang, config.stt.prefer_online, ibm_data).await?;
        info!("Using stt {}", stt.get_info());

        let voice_prefs: VoiceDescr = VoiceDescr {
            gender:if config.tts.prefer_male{Gender::Male}else{Gender::Female}
        };
        let ibm_tts = config.tts.ibm.clone();

        let mut tts = TtsFactory::load_with_prefs(&self.curr_lang, config.tts.prefer_online, ibm_tts, &voice_prefs)?;
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
                            let output = encode::to_vec(&MsgWelcome{conf:config.to_client_conf(), satellite: input.uuid})?;
                            client.publish("lily/satellite_welcome", QoS::AtMostOnce, false, output).await?
                        }
                        "lily/nlu_process" => {
                            let msg_nlu: MsgNluVoice = decode::from_read(std::io::Cursor::new(pub_msg.payload))?;
                            let (as_raw, _) = decode_ogg_opus(msg_nlu.audio)?;
                            stt.process(&as_raw).await?;
                            
                            if msg_nlu.is_final {
                                let satellite = msg_nlu.satellite;
                                LAST_SITE.with(|s|*s.borrow_mut() = Some(satellite));
                                if let Err(e) = order.received_order(
                                    stt.end_decoding().await?, 
                                    signal_event.clone(),
                                    base_context).await {

                                    error!("Actions processing had an error: {}", e);
                                }
                                
                            }
                            
                        }
                        "lily/event" => {
                            let msg: MsgEvent = decode::from_read(std::io::Cursor::new(pub_msg.payload))?;
                            let satellite = msg.satellite;
                            LAST_SITE.with(|s|*s.borrow_mut() = Some(satellite));
                            signal_event.lock().unwrap().call(&msg.event, base_context);
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

