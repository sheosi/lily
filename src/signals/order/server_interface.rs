use std::cell::RefCell;
use std::collections::{HashMap, hash_map::Entry};
use std::mem::replace;
use std::sync::{Arc, Mutex, Weak};
use std::ops::DerefMut;

use crate::actions::ActionContext;
use crate::config::Config;
use crate::nlu::{NluManager, NluManagerConf, NluManagerStatic};
use crate::python::add_context_data;
use crate::signals::{SignalEventShared, SignalOrder};
use crate::stt::{SttPool, SttPoolItem, SttSet};
use crate::tts::{Gender, TtsFactory, VoiceDescr};
use crate::vars::{DEFAULT_SAMPLES_PER_SECOND, POISON_MSG};

use anyhow::{anyhow, Result};
use lily_common::audio::{Audio, decode_ogg_opus};
use lily_common::communication::*;
use log::{error, info, warn};
use rmp_serde::{decode, encode};
use rumqttc::{Event, Packet, QoS};
use unic_langid::LanguageIdentifier;

thread_local!{
    pub static MSG_OUTPUT: RefCell<Option<MqttInterfaceOutput>> = RefCell::new(None);
    pub static CAPS_MANAGER: RefCell<CapsManager> = RefCell::new(CapsManager::new());
}


struct Session {
    device: String
}
impl Session {
    fn new(device: String) -> Self {
        Self {device}
    }
}

struct SessionManager {
    sessions: HashMap<String, Arc<Session>>,
    curr_utt_stt: HashMap<String, SttPoolItem>,
    sttset: SttSet
}

impl SessionManager {
    fn new(sttset: SttSet) -> Self {
        Self {
            sessions: HashMap::new(),
            curr_utt_stt: HashMap::new(),
            sttset
        }
    }

    fn session_for(&mut self, uuid: String) -> Weak<Session> {
        match self.sessions.entry(uuid.clone()) {
            Entry::Occupied(o) => {
                Arc::downgrade(o.get())
            }
            Entry::Vacant(v) => {
                let arc = Arc::new(Session::new(uuid));
                Arc::downgrade(v.insert(arc))
            }
        }
    }

    async fn get_stt(&mut self, uuid: String, audio:&[i16]) -> Result<&mut SttPoolItem> {
        match self.curr_utt_stt.entry(uuid) {
            Entry::Occupied(o) => Ok(o.into_mut()),
            Entry::Vacant(v) => {
                let mut stt = self.sttset.guess_stt(audio).await?;
                stt.begin_decoding().await?;
                Ok(v.insert(stt))
            }
        }
    }

    fn end_session(&mut self, uuid: &str) -> Result<()> {
        match self.sessions.remove(uuid)  {
            Some(_) => {Ok(())}
            None => {Err(anyhow!("{} had no active session", uuid))}
        }
    }
    fn end_utt(&mut self, uuid: &str) -> Result<()> {
        match self.curr_utt_stt.remove(uuid)  {
            Some(_) => {Ok(())}
            None => {Err(anyhow!("{} had no active session", uuid))}
        }
    }
}

pub struct CapsManager {
    // For now just a map of capabilities, which is a map in which if exists is true
    clients_caps: HashMap<String, HashMap<String,()>>
}

impl CapsManager {
    fn new() -> Self {
        Self {
            clients_caps: HashMap::new(),
        }
    }

    fn add_client(&mut self, uuid: &str, caps: Vec<String>) {
        let mut caps_map = HashMap::new();
        for cap in caps {
            caps_map.insert(cap, ());
        }
        
        self.clients_caps.insert(uuid.to_owned(), caps_map);
    }

    pub fn has_cap(&self, uuid: &str, cap_name: &str) -> bool {
        match self.clients_caps.get(uuid) {
            Some(client) => client.get(cap_name).map(|_|true).unwrap_or(false),
            None => false
        }
        
    }

    fn disconnected(&mut self, uuid: &str) -> Result<()> {
        match self.clients_caps.remove(uuid) {
            Some(_) => Ok(()),
            None => Err(anyhow!(format!("Satellite {} asked for a disconnect but was not connected", uuid)))
        }

    }

}

pub struct MqttInterface {
    common_out: Arc<Mutex<Vec<(SendData, String)>>>,
    curr_langs: Vec<LanguageIdentifier>
}



impl MqttInterface {
    pub fn new(curr_langs: &Vec<LanguageIdentifier>) -> Result<Self> {
        let common_out = Arc::new(Mutex::new(vec![]));
        let output = MqttInterfaceOutput::create(common_out.clone())?;
        MSG_OUTPUT.with(|a|a.replace(Some(output)));

        Ok(Self {
            common_out,
            curr_langs: curr_langs.to_owned(),
        })
    }


    pub async fn interface_loop<M: NluManager + NluManagerConf + NluManagerStatic> (&mut self, config: &Config, signal_event: SignalEventShared, base_context: &ActionContext, order: &mut SignalOrder<M>) -> Result<()> {
        let ibm_data = config.stt.ibm.clone();
        let mqtt_conf = ConnectionConfResolved::from(
            config.mqtt.clone(),
            || "lily-server".into()
        );
        let (client, mut eloop) = make_mqtt_conn(&mqtt_conf, None)?;
       

        client.subscribe("lily/new_satellite", QoS::AtMostOnce).await?;
        client.subscribe("lily/nlu_process", QoS::AtMostOnce).await?;
        client.subscribe("lily/event", QoS::AtMostOnce).await?;
        client.subscribe("lily/disconnected", QoS::ExactlyOnce).await?;

        let mut stt_set = SttSet::new();
        for lang in &self.curr_langs {
            let pool= SttPool::new(1, 1,lang, config.stt.prefer_online, &ibm_data).await?;
            stt_set.add_lang(lang, pool).await?;
        }
        let mut sessions = SessionManager::new(stt_set);
        

        let voice_prefs: VoiceDescr = VoiceDescr {
            gender:if config.tts.prefer_male{Gender::Male}else{Gender::Female}
        };
        
        let mut tts_set = HashMap::new();
        let mut def_lang = None;
        for (i,lang) in self.curr_langs.iter().enumerate() {
            let tts = TtsFactory::load_with_prefs(lang, config.tts.prefer_online, config.tts.ibm.clone(), &voice_prefs)?;
            info!("Using tts {}", tts.get_info());
            tts_set.insert(lang, tts);

            if i == 0 {def_lang = Some(lang);}
        }

        loop {
            let notification = eloop.poll().await?;
            println!("Notification = {:?}", notification);
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
                            client.publish("lily/satellite_welcome", QoS::AtMostOnce, false, output).await?
                        }
                        "lily/nlu_process" => {
                            let msg_nlu: MsgNluVoice = decode::from_read(std::io::Cursor::new(pub_msg.payload))?;
                            let (as_raw, _) = decode_ogg_opus(msg_nlu.audio, DEFAULT_SAMPLES_PER_SECOND)?;
                            let session = sessions.session_for(msg_nlu.satellite.clone());
                            {
                                let stt = sessions.get_stt(msg_nlu.satellite.clone(), &as_raw).await?;

                                stt.process(&as_raw).await?;
                                if msg_nlu.is_final {
                                    let satellite = msg_nlu.satellite.clone();
                                    let context = add_context_data(&base_context, stt.lang(), &satellite);
                                    if let Err(e) = order.received_order(
                                        stt.end_decoding().await?, 
                                        signal_event.clone(),
                                        &context,
                                        stt.lang()
                                    ).await {

                                        error!("Actions processing had an error: {}", e);
                                    }
                                    
                                }
                            }
                            if msg_nlu.is_final {
                                if let Err(e) = sessions.end_utt(&msg_nlu.satellite) {
                                    warn!("{}",e);
                                }
                            }
                            
                        }
                        "lily/event" => {
                            let msg: MsgEvent = decode::from_read(std::io::Cursor::new(pub_msg.payload))?;
                            let context = add_context_data(base_context,&self.curr_langs[0], &msg.satellite);
                            signal_event.lock().expect(POISON_MSG).call(&msg.event, &context);
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
            {
                let msg_vec = replace(self.common_out.lock().expect(POISON_MSG).deref_mut(), Vec::new());
                for (msg_data, uuid_str) in msg_vec {

                    let audio_data = match msg_data {
                        SendData::Audio(audio) => {
                            audio
                        }
                        SendData::String((str, lang)) => {
                            match tts_set.get_mut(&lang) {
                                Some(tts) => {
                                    tts.synth_text(&str).await?
                                }
                                None => {
                                    warn!("Received answer for language {:?} not in the config or that has no TTS, using default", lang);
                                    let def = def_lang.as_ref().expect("There's no language assigned, need one at least");
                                    match tts_set.get_mut(def) {
                                        Some(tts) => {
                                            tts.synth_text(&str).await?
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

                    if let Err(e) = sessions.end_session(&uuid_str) {
                        warn!("{}",e);
                    }
                    let msg_pack = encode::to_vec(&MsgAnswerVoice{data: audio_data.into_encoded()?})?;
                    client.publish(format!("lily/{}/say_msg", uuid_str), QoS::AtMostOnce, false, msg_pack).await?;
                }
            }
        }
    }
}

enum SendData {
    String((String, LanguageIdentifier)),
    Audio(Audio)
}
pub struct MqttInterfaceOutput {
    client: Arc<Mutex<Vec<(SendData, String)>>>
}

impl MqttInterfaceOutput {
    fn create(client: Arc<Mutex<Vec<(SendData, String)>>>) -> Result<Self> {
        Ok(Self{client})
    }

    pub fn answer(&mut self, input: &str, lang: &LanguageIdentifier, to: String) -> Result<()> {
        self.client.lock().expect(POISON_MSG).push((SendData::String((input.into(), lang.to_owned())), to));
        Ok(())
    }

    pub fn send_audio(&mut self, audio: Audio, to: String) -> Result<()> {
        self.client.lock().expect(POISON_MSG).push((SendData::Audio(audio), to));
        Ok(())
    }
}

