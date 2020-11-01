use std::rc::Rc;
use std::time::Duration;
use std::thread::sleep;

use lily_common::audio::{Audio, AudioRaw, PlayDevice, RecDevice, Recording};
use lily_common::hotword::{HotwordDetector, Snowboy};
use lily_common::communication::{MsgAnswerVoice, MsgNluVoice};
use lily_common::other::init_log;
use lily_common::vad::{SnowboyVad, Vad, VadError};
use lily_common::vars::*;
use log::warn;
use rmp_serde::{decode, encode};
use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Packet, QoS};
use tokio::{try_join, sync::{Mutex, MutexGuard}};
use url::Url;

const ENERGY_SAMPLING_TIME_MS: u64 = 500;



enum ProgState<'a> {
    PasiveListening,
    ActiveListening(MutexGuard<'a,RecDevice>),
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

struct ClientConf {
    hotword_sensitivity: f32
}

impl Default for ClientConf {
    fn default() -> Self {
        Self{
            hotword_sensitivity: DEFAULT_HOTWORD_SENSITIVITY
        }
    }
}


async fn send_audio<'a>(client: &mut AsyncClient,data: AudioRef<'a>, is_final: bool)-> anyhow::Result<()> {
    let msgpack_data = MsgNluVoice{
        audio: data.into_owned().to_ogg_opus()?,
        is_final
    };
    let msg_pack = encode::to_vec(&msgpack_data).unwrap();
    client.publish("lily/nlu_process", QoS::AtMostOnce, false, msg_pack).await?;
    Ok(())
}

fn record_env(mut rec_dev: MutexGuard<RecDevice>) -> anyhow::Result<AudioRaw> {
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

async fn receive (rec_dev: Rc<Mutex<RecDevice>>,eloop: &mut EventLoop) -> anyhow::Result<()> {
    let mut play_dev = PlayDevice::new().unwrap();

    loop {
        let sps =  DEFAULT_SAMPLES_PER_SECOND;

        match eloop.poll().await.unwrap() {
            Event::Incoming(Packet::Publish(pub_msg)) => {
                match pub_msg.topic.as_str() {
                    "lily/say_msg" => {
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

async fn user_listen(rec_dev: Rc<Mutex<RecDevice>>,config: &ClientConf, mut client: &mut AsyncClient) -> anyhow::Result<()> {
    
    // TODO: Send audio sample
    let _env_sample = record_env(rec_dev.lock().await)?;
    
    rec_dev.lock().await.start_recording().expect(AUDIO_REC_START_ERR_MSG);
    let mut pas_listener = {
        let snowboy_path = SNOWBOY_DATA_PATH.resolve();
        let hotword_det = Snowboy::new(&snowboy_path.join("lily.pmdl"), &snowboy_path.join("common.res"), config.hotword_sensitivity)?;
        PasiveListener::new(hotword_det)?
    };
    let vad = SnowboyVad::new(&SNOWBOY_DATA_PATH.resolve())?;
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
                        send_audio(&mut client, data, false).await?
                    }
                    ActiveState::Done(data) => {
                        send_audio(&mut client, data, true).await?;

                        current_state = ProgState::PasiveListening;

                    }
                }
                
            }
        }
    }
}

struct ConnectionConf {
    url_str: String
}

impl Default for ConnectionConf {
    fn default() -> Self {
        Self {
            url_str: "127.0.0.1:1883".to_owned()
        }
    }
}

#[tokio::main(flavor = "current_thread")]
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

    let (mut client, mut eloop) = AsyncClient::new(mqttoptions, 10);
    client.subscribe("lily/say_msg", QoS::AtMostOnce).await.unwrap();

    let config = ClientConf::default();
    
    init_log();
    // Record environment to get minimal energy threshold
    let rec_dev = Rc::new(Mutex::new(RecDevice::new().unwrap()));


    try_join!(user_listen(rec_dev.clone(), &config, &mut client),receive(rec_dev, &mut eloop)).unwrap();
}
