use std::fs::File;
use std::cell::RefCell;
use std::io::{BufReader, BufWriter};
use std::path::Path;
use std::rc::Rc;

use anyhow::anyhow;
use lily_common::audio::{Audio, AudioRaw, PlayDevice, RecDevice};
use lily_common::communication::*;
use lily_common::hotword::{HotwordDetector, Snowboy};
use lily_common::other::{ConnectionConf, init_log};
use lily_common::vad::{SnowboyVad, Vad, VadError};
use lily_common::vars::*;
use log::{debug, info};
use rmp_serde::{decode, encode};
use rumqttc::{AsyncClient, Event, EventLoop, Packet, QoS};
use serde::{Deserialize, Serialize};
use serde_yaml::{from_reader,to_writer};
use tokio::{try_join, sync::{Mutex as AsyncMutex, MutexGuard as AsyncMGuard}};
use tokio::sync::watch;
use uuid::Uuid;

const CONN_CONF_FILE: PathRef = PathRef::new("conn_conf.yaml");

enum ProgState<'a> {
    PasiveListening,
    ActiveListening(AsyncMGuard<'a,RecDevice>),
}

impl<'a> PartialEq for ProgState<'a> {
    fn eq(&self, other: &Self) -> bool {
        match self {
            ProgState::PasiveListening =>{
                match other {
                    ProgState::PasiveListening => {true}
                    _ => {false}
                }
            }
            ProgState::ActiveListening(_) =>{
                match other {
                    ProgState::ActiveListening(_) => {true}
                    _ => {false}
                }
            }
        }
    }
}
struct ActiveListener<V:Vad> {
    was_talking: bool,
    vad: V
}

struct PasiveListener<H: HotwordDetector> {
    hotword_detector: H
}

impl<H: HotwordDetector> PasiveListener<H> {
    fn new(mut hotword_detector: H) -> anyhow::Result<Self> {
        hotword_detector.start_hotword_check()?;
        Ok(Self {hotword_detector})
    }

    fn process(&mut self, audio: AudioRef) -> anyhow::Result<bool> {
        self.hotword_detector.check_hotword(audio.data)
    }

    fn set_from_conf(&mut self, conf: &ClientConf) {
        self.hotword_detector.set_sensitivity(conf.hotword_sensitivity)
    }
}

enum ActiveState<'a> {
    // TODO: Add timeout
    NoOneTalking,
    Hearing(AudioRef<'a>),
    Done(AudioRef<'a>)
}

impl<V: Vad> ActiveListener<V> {
    fn new(vad: V) -> Self {
        Self {was_talking: false,vad}
    }

    fn process<'a>(&mut self, audio: AudioRef<'a>) -> Result<ActiveState<'a>, VadError> {
        if self.vad.is_someone_talking(audio.data)? {
            self.was_talking = true;
            Ok(ActiveState::Hearing(audio))
        }
        else {
            if self.was_talking {
                self.vad.reset()?;
                self.was_talking = false;
                Ok(ActiveState::Done(audio))
            }
            else {
                Ok(ActiveState::NoOneTalking)
            }
        }
    }
}

#[derive(Clone)]
struct AudioRef<'a> {
    data: &'a [i16]
}

impl<'a> AudioRef<'a> {
    fn from(data: &'a[i16]) -> Self {
        Self{data}
    }

    fn into_owned(self) -> AudioRaw {
        AudioRaw::new_raw(self.data.to_owned(), DEFAULT_SAMPLES_PER_SECOND)
    }
}


async fn send_audio<'a>(mqtt_name: &str, client: Rc<RefCell<AsyncClient>>,data: AudioRef<'a>, is_final: bool)-> anyhow::Result<()> {
    let msgpack_data = MsgNluVoice{
        audio: data.into_owned().to_ogg_opus()?,
        is_final,
        satellite: mqtt_name.to_owned()
    };
    let msg_pack = encode::to_vec(&msgpack_data).unwrap();
    client.borrow_mut().publish("lily/nlu_process", QoS::AtMostOnce, false, msg_pack).await?;
    Ok(())
}

async fn receive ( 
    my_name: &str, 
    rec_dev: Rc<AsyncMutex<RecDevice>>,
    conf_change: watch::Sender<ClientConf>,
    client:Rc<RefCell<AsyncClient>>,
    eloop: &mut EventLoop
) -> anyhow::Result<()> {

    let mut play_dev = PlayDevice::new().unwrap();
    // We will be listening from now on, say hello
    let msg_pack = encode::to_vec(&MsgNewSatellite{uuid: my_name.to_string()})?;
    client.borrow_mut().publish("lily/new_satellite", QoS::AtLeastOnce, false, msg_pack).await?;
    loop {
        let sps =  DEFAULT_SAMPLES_PER_SECOND;
        let a = eloop.poll().await.unwrap();
        match  a {
            Event::Incoming(Packet::Publish(pub_msg)) => {
                let topic = pub_msg.topic.as_str();
                match  topic {
                    "lily/satellite_welcome" => {
                        info!("Received config from server");
                        let msg :MsgWelcome = decode::from_read(std::io::Cursor::new(pub_msg.payload))?;
                        if msg.satellite == my_name {
                            let client_conf_srvr = msg.conf;
                            conf_change.send(client_conf_srvr)?;
                        }
                    }
                    _ if topic.ends_with("/say_msg") => {
                        let msg: MsgAnswerVoice = decode::from_read(std::io::Cursor::new(pub_msg.payload)).unwrap();
                        let audio = Audio::new_encoded(msg.data, sps);
                        {
                            // Take unique ownership of the record device while playing something
                            let mut rec_mut = rec_dev.lock().await;
                            rec_mut.stop_recording().expect(AUDIO_REC_STOP_ERR_MSG);
                            play_dev.play_audio(audio)?;
                            rec_mut.start_recording().expect(AUDIO_REC_START_ERR_MSG);
                        }
                    }
                    _ => {}
                }
            }
            Event::Incoming(_) => {}
            Event::Outgoing(_) => {}
        }
        
    }
}

struct DebugAudio {
    audio: AudioRaw,
    save_ms: u16,
    curr_ms: f32
}

impl DebugAudio {
    fn new(save_ms: u16) -> Self {
        Self{audio: AudioRaw::new_empty(DEFAULT_SAMPLES_PER_SECOND), save_ms, curr_ms:0.0}
    }

    fn push(&mut self, audio: &AudioRef) {
        self.curr_ms += (audio.data.len() as f32)/(DEFAULT_SAMPLES_PER_SECOND as f32) * 1000.0;
        self.audio.append_audio(audio.data, DEFAULT_SAMPLES_PER_SECOND);
        if (self.curr_ms as u16) >= self.save_ms {
            println!("Save to file");
            self.audio.save_to_disk(Path::new("debug.ogg")).expect("Failed to write debug file");
            self.clear();
        }
    }

    fn clear(&mut self) {
        self.audio.clear();
        self.curr_ms = 0.0;
    }
}

async fn user_listen(mqtt_name: &str ,rec_dev: Rc<AsyncMutex<RecDevice>>, mut config: watch::Receiver<ClientConf> , client: Rc<RefCell<AsyncClient>>) -> anyhow::Result<()> {
    let snowboy_path = SNOWBOY_DATA_PATH.resolve();
    let mut pas_listener = {
        let hotword_det = Snowboy::new(&snowboy_path.join("lily.pmdl"), &snowboy_path.join("common.res"), config.borrow().hotword_sensitivity)?;
        PasiveListener::new(hotword_det)?
    };
    
    let vad = SnowboyVad::new(&snowboy_path.join("common.res"))?;
    let mut act_listener = ActiveListener::new(vad);
    let mut current_state = ProgState::PasiveListening;

    let mut debugaudio = DebugAudio::new(2000);
    rec_dev.lock().await.start_recording().expect(AUDIO_REC_START_ERR_MSG);
    loop {
        let interval =
            if current_state == ProgState::PasiveListening {HOTWORD_CHECK_INTERVAL_MS}
            else {ACTIVE_LISTENING_INTERVAL_MS};

        match current_state {
            ProgState::PasiveListening => {

                let mut rec_guard = rec_dev.lock().await;
                tokio::select! {
                    conf = config.changed() => {
                        conf?;
                        pas_listener.set_from_conf(&config.borrow());
                    }
                    r = rec_guard.read_for_ms(interval) => {
                        match r? {
                            Some(d) => {
                                let mic_data = AudioRef::from(d);
                                debugaudio.push(&mic_data);
    
                                if pas_listener.process(mic_data)? {
                                    current_state = ProgState::ActiveListening(rec_guard);
                
                                    debug!("I'm listening for your command");
                
                                    let msg_pack = encode::to_vec(&MsgEvent{satellite: mqtt_name.to_string(), event: "init_reco".into()})?;
                                    client.borrow_mut().publish("lily/event", QoS::AtMostOnce, false, msg_pack).await?;
                                }
    
                            }
                            None => ()
                        };
                    }
                }
                
            }
            ProgState::ActiveListening(ref mut rec_guard) => {
                let mic_data = match rec_guard.read_for_ms(interval).await? {
                    Some(d) => AudioRef::from(d),
                    None => continue
                };

                match act_listener.process(mic_data)? {
                    ActiveState::NoOneTalking => {}
                    ActiveState::Hearing(data) => {
                        send_audio(mqtt_name.into(), client.clone(), data, false).await?
                    }
                    ActiveState::Done(data) => {
                        send_audio(mqtt_name.into(), client.clone(), data, true).await?;

                        current_state = ProgState::PasiveListening;
                    }
                }
                
            }
        }
    }
}

#[derive(Clone, Deserialize, Debug, Serialize)]
struct ConfFile {
    #[serde(default)]
    mqtt: ConnectionConf
}

impl Default for ConfFile {
    fn default() -> Self {
        Self {
            mqtt: ConnectionConf::default()
        }
    }
}
impl ConfFile {
    fn load() -> anyhow::Result<ConfFile>{
        let conf_path = CONN_CONF_FILE.resolve();
        if conf_path.is_file()  {
            let conf_file = File::open(conf_path)?;
            Ok(from_reader(BufReader::new(conf_file))?)
        }
        else {
            Err(anyhow!("Config file not found"))
        }
    }

    fn save(&mut self) -> anyhow::Result<()> {
        let conf_path = CONN_CONF_FILE.resolve();
        let conf_file = File::create(&conf_path)?;
        let writer = BufWriter::new(&conf_file);
        Ok(to_writer(writer, &self)?)
    }
}


#[tokio::main(flavor="current_thread")]
pub async fn main() -> anyhow::Result<()> {

    init_log("lily-client".into());
    
    let config = ConfFile::load().unwrap_or(ConfFile::default());
    let mqtt_conn = {
        let mut old_conf = config.clone();
        let conn = ConnectionConfResolved::from(
            config.mqtt, 
            ||format!("lily-client-{}", Uuid::new_v4().to_string())
        );

        if old_conf.mqtt.name.is_none() {
            old_conf.mqtt.name = Some(conn.name.clone());
            old_conf.save()?;
        }

        conn
    };

    let (client, mut eloop) = make_mqtt_conn(&mqtt_conn);

    client.subscribe("lily/say_msg", QoS::AtMostOnce).await?;
    client.subscribe("lily/satellite_welcome", QoS::AtMostOnce).await?;
    let client_share = Rc::new(RefCell::new(client));

    info!("Mqtt connection made");

    let (conf_change_tx, conf_change_rx) = watch::channel(ClientConf::default());
    let rec_dev = Rc::new(AsyncMutex::new(RecDevice::new()));
    try_join!(
        receive(&mqtt_conn.name, rec_dev.clone(), conf_change_tx, client_share.clone(), &mut eloop),
        user_listen(&mqtt_conn.name, rec_dev, conf_change_rx, client_share, )
    ).unwrap();

    Ok(())
}
