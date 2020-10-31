use std::time::Duration;
use std::thread::sleep;

use rumqttc::{Connection, Client, Event, MqttOptions, Packet, QoS};
use url::Url;
use rmp_serde::{decode, encode};
use lily_common::audio::{Audio, AudioRaw, PlayDevice, RecDevice, Recording};
use lily_common::hotword::{HotwordDetector, Snowboy};
use lily_common::communication::{MsgAnswerVoice, MsgNluVoice};
use lily_common::other::init_log;
use lily_common::vad::{SnowboyVad, Vad, VadError};
use lily_common::vars::*;
use log::warn;

const ENERGY_SAMPLING_TIME_MS: u64 = 500;

#[derive(PartialEq)]
enum ProgState {
    PasiveListening,
    ActiveListening,
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

fn wait_for_answer(connection: &mut Connection) -> Audio {
    let sps =  DEFAULT_SAMPLES_PER_SECOND;
    for notification in connection.iter() {

        match notification.unwrap() {
            Event::Incoming(Packet::Publish(pub_msg)) => {
                match pub_msg.topic.as_str() {
                    "lily/say_msg" => {
                        let msg: MsgAnswerVoice = decode::from_read(std::io::Cursor::new(pub_msg.payload)).unwrap();
                        return Audio::new_encoded(msg.data, sps);
                    }
                    _ => {}
                }
            }
            Event::Incoming(_) => {}
            Event::Outgoing(_) => {}
        }
    }

    Audio::new_empty(sps)
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


fn send_audio(client: &mut Client,data: AudioRef, is_final: bool)-> anyhow::Result<()> {
    let msgpack_data = MsgNluVoice{
        audio: data.into_owned().to_ogg_opus()?,
        is_final
    };
    let msg_pack = encode::to_vec(&msgpack_data).unwrap();
    client.publish("lily/nlu_process", QoS::AtMostOnce, false, msg_pack).unwrap();
    Ok(())
}

fn record_env(rec_dev: &mut RecDevice) -> anyhow::Result<AudioRaw> {
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

fn main() -> anyhow::Result<()> {
    let url_str = "127.0.0.1:1883";
    let url = Url::parse(
        &format!("http://{}",url_str) // Let's add some protocol
    ).unwrap();
    let host = url.host_str().unwrap();
    let port: u16 = url.port().unwrap_or(1883);


    // TODO: Set username and passwd

    // Init MQTT
    let mut mqttoptions = MqttOptions::new("lily-client", host, port);
    mqttoptions.set_keep_alive(5);

    let (mut client, mut connection) = Client::new(mqttoptions, 10);
    client.subscribe("lily/say_msg", QoS::AtMostOnce).unwrap();

    let config = ClientConf::default();

    let vad = SnowboyVad::new(&SNOWBOY_DATA_PATH.resolve())?;
    let mut act_listener = ActiveListener::new(vad);
    let mut rec_dev = RecDevice::new()?;
    let mut play_dev = PlayDevice::new()?;

    let mut current_state = ProgState::PasiveListening;
    rec_dev.start_recording().expect(AUDIO_REC_START_ERR_MSG);
    let mut pas_listener = {
        let snowboy_path = SNOWBOY_DATA_PATH.resolve();
        let hotword_det = Snowboy::new(&snowboy_path.join("lily.pmdl"), &snowboy_path.join("common.res"), config.hotword_sensitivity)?;
        PasiveListener::new(hotword_det)?
    };
    
    init_log();

     // Record environment to get minimal energy threshold
     let mut record_device = RecDevice::new()?;
     record_device.start_recording().expect(AUDIO_REC_START_ERR_MSG);
     sleep(Duration::from_millis(ENERGY_SAMPLING_TIME_MS));
     
     let _audio_sample = record_env(&mut rec_dev)?;
     // TODO: Send audio sample to server
     record_device.stop_recording()?;

    loop {
        let interval =
            if current_state == ProgState::PasiveListening {HOTWORD_CHECK_INTERVAL_MS}
            else {ACTIVE_LISTENING_INTERVAL_MS};
        
        let mic_data = match rec_dev.read_for_ms(interval)? {
            Some(d) => AudioRef::from(d),
            None => continue
        };

        match current_state {
            ProgState::PasiveListening => {
                if pas_listener.process(mic_data)? {
                    current_state = ProgState::ActiveListening;
                    // Notify change

                }
            }
            ProgState::ActiveListening => {
                match act_listener.process(mic_data)? {
                    ActiveState::NoOneTalking => {}
                    ActiveState::Hearing(data) => {
                        send_audio(&mut client, data, false)?
                    }
                    ActiveState::Done(data) => {
                        send_audio(&mut client, data, true)?;

                        current_state = ProgState::PasiveListening;
                        record_device.stop_recording().expect(AUDIO_REC_STOP_ERR_MSG);
                        play_dev.play_audio( wait_for_answer(&mut connection))?;
                        record_device.start_recording().expect(AUDIO_REC_START_ERR_MSG);
                        

                    }
                }
                
            }
        }
    }
}
