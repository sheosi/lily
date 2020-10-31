use rumqttc::{Event, MqttOptions, Client, Packet, QoS};
use url::Url;
use rmp_serde::{decode, encode};
use lily_common::communication::{MsgAnswerVoice, MsgNluVoice};
use lily_common::vad::{SnowboyVad, Vad, VadError};
use lily_common::vars::{AUDIO_REC_START_ERR_MSG, ACTIVE_LISTENING_INTERVAL_MS, HOTWORD_CHECK_INTERVAL_MS, SNOWBOY_DATA_PATH};
use lily_common::audio::{AudioRaw, RecDevice, Recording};
use lily_common::hotword::{HotwordDetector, HotwordError, Snowboy};

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
    fn new(hotword_detector: H) -> Self {
        hotword_detector.start_hotword_check();
        Self {hotword_detector}
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
                self.vad.reset();
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

    let vad = SnowboyVad::new(&SNOWBOY_DATA_PATH.resolve());
    let act_listener = ActiveListener::new(vad)?;
    let rec_dev = RecDevice::new()?;

    let current_state = ProgState::PasiveListening;
    rec_dev.start_recording().expect(AUDIO_REC_START_ERR_MSG);
    let mut hotword_detector = {
        let snowboy_path = SNOWBOY_DATA_PATH.resolve();
        Snowboy::new(&snowboy_path.join("lily.pmdl"), &snowboy_path.join("common.res"), config.hotword_sensitivity)?
    };
    let pas_listener = PasiveListener::new(hotword_detector);

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
                act_listener.process(mic_data)
                
            }
        }
        
        let msg_pack = encode::to_vec(&MsgNlu{hypothesis: "test".to_string()}).unwrap();
        client.publish("lily/nlu_process", QoS::AtMostOnce, false, msg_pack).unwrap();


        for notification in connection.iter() {

            match notification.unwrap() {
                Event::Incoming(Packet::Publish(pub_msg)) => {
                    match pub_msg.topic.as_str() {
                        "lily/say_msg" => {
                            let msg: MsgAnswer = decode::from_read(std::io::Cursor::new(pub_msg.payload)).unwrap();
                            println!("{}", msg.data);

                            break; 
                        }
                        _ => {}
                    }
                }
                Event::Incoming(_) => {}
                Event::Outgoing(_) => {}
            }
        }
    }
}
