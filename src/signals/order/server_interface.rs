use std::cell::RefCell;
use std::collections::{HashMap, hash_map::Entry};
use std::io::Cursor;
use std::fmt::Debug;
use std::sync::{Arc, Mutex, Weak};

use crate::{actions::ActionContext, stt::DecodeRes};
use crate::config::Config;
use crate::nlu::{NluManager, NluManagerConf, NluManagerStatic};
use crate::signals::{process_answers, SignalEventShared, SignalOrder};
use crate::stt::{SttPool, SttPoolItem, SttSet};
use crate::tts::{Gender, Tts, TtsFactory, VoiceDescr};
use crate::vars::POISON_MSG;

use anyhow::{anyhow, Result};
use lily_common::audio::{Audio, AudioRaw, decode_ogg_opus};
use lily_common::communication::*;
use lily_common::vars::{DEFAULT_SAMPLES_PER_SECOND, PathRef};
use log::{debug, error, info, warn};
use rmp_serde::{decode, encode};
use rumqttc::{AsyncClient, Event, EventLoop, Packet, QoS};
use tokio::{try_join, sync::mpsc};
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
struct UtteranceManager {
    curr_utt_stt: HashMap<String, SttPoolItem>,
    sttset: SttSet
}
impl UtteranceManager {
    pub fn new(sttset: SttSet) -> Self {
        Self{curr_utt_stt: HashMap::new(), sttset}
    }

    async fn get_stt(&mut self, uuid: String, audio:&[i16]) -> Result<&mut SttPoolItem> {
        match self.curr_utt_stt.entry(uuid) {
            Entry::Occupied(o) => Ok(o.into_mut()),
            Entry::Vacant(v) => {
                let mut stt = self.sttset.guess_stt(audio).await?;
                debug!("STT for current session: {}", stt.get_info());
                stt.begin_decoding().await?;
                Ok(v.insert(stt))
            }
        }
    }
    fn end_utt(&mut self, uuid: &str) -> Result<()> {
        match self.curr_utt_stt.remove(uuid)  {
            Some(_) => {Ok(())}
            None => {Err(anyhow!("{} had no active session", uuid))}
        }
    }
}

pub struct SessionManager {
    sessions: HashMap<String, Arc<Session>>
}

impl SessionManager {
    pub fn new() -> Self {
        Self {sessions: HashMap::new()}
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

    fn end_session(&mut self, uuid: &str) -> Result<()> {
        match self.sessions.remove(uuid)  {
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

pub fn add_context_data(dict: &ActionContext, lang: &LanguageIdentifier, client: &str) -> ActionContext {
    
    let mut new = dict.copy();
    new.set_str("locale".into(), lang.to_string());
    let mut satellite = ActionContext::new();
    satellite.set_str("uuid".to_string(), client.to_string());
    new.set_dict("satellite".into(), satellite);

    new
}

pub struct MqttInterface {
    common_out: mpsc::Receiver<(SendData, String)>
}


mod language_detection {
    // use unic_langid::LanguageIdentifier;
    //use lingua::{Language, LanguageDetector, LanguageDetectorBuilder};

    /*fn id_to_lingua() -> lingua::La{

    }*/

    /*let languages = vec![English, French, German, Spanish];
    let detector: LanguageDetector = LanguageDetectorBuilder::from_languages(&languages).build();
    let detected_language: Option<Language> = detector.detect_language_of("languages are awesome");*/
}

pub async fn on_nlu_request<M: NluManager + NluManagerConf + NluManagerStatic + Debug + Send + 'static>(
    config: &Config,
    mut channel: mpsc::Receiver<MsgRequest>,
    signal_event: SignalEventShared,
    curr_langs: &Vec<LanguageIdentifier>,
    order: &mut SignalOrder<M>,
    base_context: &ActionContext,
    sessions: Arc<Mutex<SessionManager>>
) -> Result<()> {
    let mut stt_set = SttSet::new();
    let ibm_data = config.stt.ibm.clone();
    for lang in curr_langs {
        let pool= SttPool::new(1, 1,lang, config.stt.prefer_online, &ibm_data).await?;
        stt_set.add_lang(lang, pool).await?;
    }
    let mut utterances = UtteranceManager::new(stt_set);

    let mut stt_audio = AudioRaw::new_empty(DEFAULT_SAMPLES_PER_SECOND);
    let audio_debug_path = PathRef::user_cfg("stt_audio.ogg").resolve();

    loop {
        let msg_nlu = channel.recv().await.expect("Channel closed!");
        match msg_nlu.data {
            RequestData::Text(text) => {
                let lang = &curr_langs[0];
                let context = add_context_data(&base_context, lang, &msg_nlu.satellite);
                let decoded = Some(DecodeRes{hypothesis: text, confidence: 1.0});

                if let Err(e) = order.received_order(
                    decoded, 
                    signal_event.clone(),
                    &context,
                    lang,
                    msg_nlu.satellite
                ).await {

                    error!("Actions processing had an error: {}", e);
                }
            }
            RequestData::Audio{data: audio, is_final} => {
                let (as_raw, _, _) = decode_ogg_opus::<_, DEFAULT_SAMPLES_PER_SECOND>(Cursor::new(audio))?;

                if cfg!(debug_assertions) {
                    stt_audio.append_audio(&as_raw, DEFAULT_SAMPLES_PER_SECOND)?;
                }

                let session = sessions.lock().expect(POISON_MSG).session_for(msg_nlu.satellite.clone());
                {
                    match utterances.get_stt(msg_nlu.satellite.clone(), &as_raw).await {
                        Ok(stt) => {
                            if let Err(e) = stt.process(&as_raw).await {
                                error!("Stt failed to process audio: {}", e);
                            }

                            else if is_final {
                                if cfg!(debug_assertions) {
                                    stt_audio.save_to_disk(&audio_debug_path)?;
                                    stt_audio.clear();
                                }
                                let satellite = msg_nlu.satellite.clone();
                                let context = add_context_data(&base_context, stt.lang(), &satellite);
                                match stt.end_decoding().await {
                                    Ok(decoded)=> {
                                        if let Err(e) = order.received_order(
                                            decoded, 
                                            signal_event.clone(),
                                            &context,
                                            stt.lang(),
                                            satellite
                                        ).await {

                                            error!("Actions processing had an error: {}", e);
                                        }
                                    }
                                    Err(e) => error!("Stt failed while doing final decode: {}", e)
                                }
                                
                            }
                        }
                        Err(e) => {
                            error!("Failed to obtain Stt for this session: {}", e);
                        }
                    }
                }
                if is_final {
                    if let Err(e) = utterances.end_utt(&msg_nlu.satellite) {
                        warn!("{}",e);
                    }
                }
            }
        }
    }
}

pub async fn on_event(
    mut channel: mpsc::Receiver<MsgEvent>,
    signal_event: SignalEventShared,
    def_lang: Option<&LanguageIdentifier>,
    base_context: &ActionContext,
) -> Result<()> {
    let def_lang = def_lang.unwrap();
    loop {
        let msg = channel.recv().await.expect("Channel closed!");
        let context = add_context_data(base_context, &def_lang, &msg.satellite);
        let ans = signal_event.lock().expect(POISON_MSG).call(&msg.event, context.clone());
        if let Err(e) = process_answers(ans,&def_lang,msg.satellite) {
            error!("Occurred a problem while processing event: {}", e);
        }
    }
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
                                client.lock().expect(POISON_MSG).publish("lily/satellite_welcome", QoS::AtMostOnce, false, output).await?
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
            client.lock().expect(POISON_MSG).publish(&format!("lily/{}/say_msg", uuid_str), QoS::AtMostOnce, false, msg_pack).await?;
            Ok(())
        }

        let i = handle_in(eloop, client.clone(), config, channel_nlu, channel_event);
        let o = handle_out(&mut self.common_out, &mut tts_set, def_lang, sessions, client);
        try_join!(i, o)?;
                
        Ok(())
    }
}


#[derive(Debug)]
enum SendData {
    String((String, LanguageIdentifier)),
    Audio(Audio)
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

