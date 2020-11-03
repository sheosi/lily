use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Mutex;
use std::time::Duration;
use std::thread::sleep;

use lazy_static::lazy_static;
use lily_common::audio::{Audio, AudioRaw, PlayDevice, RecDevice, Recording};
use lily_common::communication::*;
use lily_common::extensions::MakeSendable;
use lily_common::hotword::{HotwordDetector, Snowboy};
use lily_common::other::init_log;
use lily_common::vad::{SnowboyVad, Vad, VadError};
use lily_common::vars::*;
use log::warn;
use rmp_serde::{decode, encode};
use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Packet, QoS};
use tokio::{try_join, sync::{Mutex as AsyncMutex, MutexGuard as AsyncMGuard}};
use url::Url;

const ENERGY_SAMPLING_TIME_MS: u64 = 500;

lazy_static! {
    static ref MY_UUID: Mutex<Option<String>> = Mutex::new(None);
}


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


async fn send_audio<'a>(client: Rc<RefCell<AsyncClient>>,data: AudioRef<'a>, is_final: bool)-> anyhow::Result<()> {
    let msgpack_data = MsgNluVoice{
        audio: data.into_owned().to_ogg_opus()?,
        is_final
    };
    let msg_pack = encode::to_vec(&msgpack_data).unwrap();
    client.borrow_mut().publish("lily/nlu_process", QoS::AtMostOnce, false, msg_pack).await?;
    Ok(())
}

fn record_env(mut rec_dev: AsyncMGuard<RecDevice>) -> anyhow::Result<AudioRaw> {
    // Record environment to get minimal energy threshold
    rec_dev.start_recording().expect(AUDIO_REC_START_ERR_MSG);
    sleep(Duration::from_millis(ENERGY_SAMPLING_TIME_MS));
    let audio_sample = {
        match rec_dev.read()? {
            Some(buffer) => {
                AudioRaw::new_raw(buffer.to_owned(), DEFAULT_SAMPLES_PER_SECOND)
            }
            None => {
                warn!("Couldn't obtain mic input data for energy sampling while loading");
                AudioRaw::new_empty(DEFAULT_SAMPLES_PER_SECOND)
            }
        }

    };

    rec_dev.stop_recording()?;
    Ok(audio_sample)
}

async fn receive (rec_dev: Rc<AsyncMutex<RecDevice>>,eloop: &mut EventLoop, my_name: &str, client:Rc<RefCell<AsyncClient>>) -> anyhow::Result<()> {
    let mut play_dev = PlayDevice::new().unwrap();
    // We will be listening from now on, say hello
    let msg_pack = encode::to_vec(&MsgNewSatellite{name: my_name.to_string()})?;
    client.borrow_mut().publish("lily/new_satellite", QoS::AtLeastOnce, false, msg_pack).await?;
    loop {
        let sps =  DEFAULT_SAMPLES_PER_SECOND;
        let a = eloop.poll().await.unwrap();
        println!("Cycle");
        match  a {
            Event::Incoming(Packet::Publish(pub_msg)) => {
                let topic = pub_msg.topic.as_str();
                match  topic {
                    "lily/satellite_welcome" => {
                        let input :MsgWelcome = decode::from_read(std::io::Cursor::new(pub_msg.payload))?;
                        if input.name == my_name {
                            let mut uuid = MY_UUID.lock().sendable()?;
                            if uuid.is_none() {
                                let as_string = input.uuid.to_string();
                                client.borrow_mut().subscribe(format!("lily/{}/say_msg", &as_string), QoS::AtMostOnce).await?;
                                uuid.replace(as_string);
                            }
                            
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

async fn user_listen(rec_dev: Rc<AsyncMutex<RecDevice>>,config: &ClientConf, client: Rc<RefCell<AsyncClient>>) -> anyhow::Result<()> {
    // TODO: Send audio sample
    let _env_sample = record_env(rec_dev.lock().await)?;
    
    rec_dev.lock().await.start_recording().expect(AUDIO_REC_START_ERR_MSG);
    let snowboy_path = SNOWBOY_DATA_PATH.resolve();
    let mut pas_listener = {
        let hotword_det = Snowboy::new(&snowboy_path.join("lily.pmdl"), &snowboy_path.join("common.res"), config.hotword_sensitivity)?;
        PasiveListener::new(hotword_det)?
    };
    
    let vad = SnowboyVad::new(&snowboy_path.join("common.res"))?;
    let mut act_listener = ActiveListener::new(vad);
    let mut current_state = ProgState::PasiveListening;
    
    loop {
        let interval =
            if current_state == ProgState::PasiveListening {HOTWORD_CHECK_INTERVAL_MS}
            else {ACTIVE_LISTENING_INTERVAL_MS};

        match current_state {
            ProgState::PasiveListening => {
                let mut rec_guard = rec_dev.lock().await;
                let mic_data = 
                    match rec_guard.read_for_ms(interval)? {
                        Some(d) => AudioRef::from(d),
                        None => continue
                };

                if pas_listener.process(mic_data)? {
                    current_state = ProgState::ActiveListening(rec_dev.lock().await);
                    // Notify change

                }
            }
            ProgState::ActiveListening(ref mut rec_guard) => {
                let mic_data = match rec_guard.read_for_ms(interval)? {
                    Some(d) => AudioRef::from(d),
                    None => continue
                };

                match act_listener.process(mic_data)? {
                    ActiveState::NoOneTalking => {}
                    ActiveState::Hearing(data) => {
                        send_audio(client.clone(), data, false).await?
                    }
                    ActiveState::Done(data) => {
                        send_audio(client.clone(), data, true).await?;

                        current_state = ProgState::PasiveListening;

                    }
                }
                
            }
        }
    }
}

struct ConnectionConf {
    url_str: String,
    name: String
}

impl Default for ConnectionConf {
    fn default() -> Self {
        Self {
            url_str: "127.0.0.1".to_owned(),
            name: "default".to_owned()
        }
    }
}

#[tokio::main]
pub async fn main() {

    
    let con_conf = ConnectionConf::default();
    let url_str = con_conf.url_str;
    let url = Url::parse(
        &format!("http://{}",url_str) // Let's add some protocol
    ).unwrap();
    let host = url.host_str().unwrap();
    let port: u16 = url.port().unwrap_or(1883);


    // TODO: Set username and passwd

    // Init MQTT
    let mut mqttoptions = MqttOptions::new("lily-client", host, port);
    mqttoptions.set_keep_alive(5);

    let (client, mut eloop) = AsyncClient::new(mqttoptions, 10);
    client.subscribe("lily/say_msg", QoS::AtMostOnce).await.unwrap();
    let client_share = Rc::new(RefCell::new(client));

    let config = ClientConf::default();
    
    init_log();

    
    // Record environment to get minimal energy threshold
    let rec_dev = Rc::new(AsyncMutex::new(RecDevice::new().unwrap()));
    try_join!(user_listen(rec_dev.clone(), &config, client_share.clone()),receive(rec_dev, &mut eloop, &con_conf.name, client_share)).unwrap();
}
