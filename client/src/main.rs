// Standard library
use std::cell::RefCell;
use std::fs::{create_dir_all, File};
use std::io::{BufReader, Cursor, BufWriter, stdin};
use std::path::Path;
use std::rc::Rc;

// Other crates
use anyhow::anyhow;
use client_components::*;
use lily_common::audio::{Audio, AudioRaw, PlayDevice, RecDevice};
use lily_common::client::{hotword::Snowboy, vad::SnowboyVad};
use lily_common::communication::*;
use lily_common::other::{init_log, ConnectionConf};
use lily_common::vars::*;
use log::{debug, info};
use ogg_opus::decode as opus_decode;
use rmp_serde::{decode, encode};
use rumqttc::{AsyncClient, Event, EventLoop, LastWill, Packet, QoS};
use serde::{Deserialize, Serialize};
use serde_yaml::{from_reader, to_writer};
use termion::{event::Key, input::TermRead};
use tokio::sync::{watch, mpsc};
use tokio::{
    sync::{Mutex as AsyncMutex, MutexGuard as AsyncMGuard},
    try_join,
};
use uuid::Uuid;

const CONN_CONF_FILE: MultipathRef = MultipathRef::new(&[
    #[cfg(debug_assertions)]
    PathRef::user_cfg("conn_conf.yaml"),
    PathRef::own("conn_conf.yaml"),
]);

const SNOWBOY_DATA_PATH: PathRef = PathRef::own("hotword");
pub const HOTWORD_CHECK_INTERVAL_MS: u16 = 20; // Larger = less CPU, more wait time
pub const ACTIVE_LISTENING_INTERVAL_MS: u16 = 200; 
enum ProgState<'a> {
    PasiveListening,
    ActiveListening(AsyncMGuard<'a, RecDevice>),
}

impl<'a> PartialEq for ProgState<'a> {
    fn eq(&self, other: &Self) -> bool {
        match self {
            ProgState::PasiveListening => matches!(other, ProgState::PasiveListening),
            ProgState::ActiveListening(_) => matches!(other, ProgState::ActiveListening(_))
        }
    }
}

async fn send_audio(
    mqtt_name: &str,
    client: Rc<RefCell<AsyncClient>>,
    data: AudioRef<'_>,
    is_final: bool,
) -> anyhow::Result<()> {
    let msgpack_data = MsgRequest {
        data: RequestData::Audio {
            data: data.into_owned().to_ogg_opus()?,
            is_final,
        },
        satellite: mqtt_name.to_owned(),
    };
    let msg_pack = encode::to_vec(&msgpack_data)?;
    client
        .borrow_mut()
        .publish("lily/nlu_process", QoS::AtMostOnce, false, msg_pack)
        .await?;
    Ok(())
}

async fn receive(
    my_name: &str,
    _rec_dev: Rc<AsyncMutex<RecDevice>>,
    conf_change: watch::Sender<ClientConf>,
    client: Rc<RefCell<AsyncClient>>,
    eloop: &mut EventLoop,
) -> anyhow::Result<()> {
    let mut play_dev = PlayDevice::new()?;
    // We will be listening from now on, say hello
    let msg_pack = encode::to_vec(&MsgNewSatellite {
        uuid: my_name.to_string(),
        caps: vec!["voice".into()],
    })?;
    client
        .borrow_mut()
        .publish("lily/new_satellite", QoS::AtLeastOnce, false, msg_pack)
        .await?;
    loop {
        match eloop.poll().await? {
            Event::Incoming(Packet::Publish(pub_msg)) => {
                let topic = pub_msg.topic.as_str();
                match topic {
                    "lily/satellite_welcome" => {
                        info!("Received config from server");
                        let msg: MsgWelcome =
                            decode::from_read(std::io::Cursor::new(pub_msg.payload))?;
                        if msg.satellite == my_name {
                            let client_conf_srvr = msg.conf;
                            conf_change.send(client_conf_srvr)?;
                        }
                    }
                    _ if topic.ends_with("/say_msg") => {
                        debug!("Received msg from server");
                        let msg: MsgAnswer =
                            decode::from_read(std::io::Cursor::new(pub_msg.payload))?;
                        msg.audio
                            .map(|b| -> Result<(), anyhow::Error> {
                                let audio = Audio::new_encoded(b);
                                {
                                    // Take unique ownership of the record device while playing something
                                    //let mut rec_mut = rec_dev.lock().await;
                                    //rec_mut.stop_recording().expect(AUDIO_REC_STOP_ERR_MSG);
                                    play_dev.play_audio(audio)?;
                                    //rec_mut.start_recording().expect(AUDIO_REC_START_ERR_MSG);
                                }
                                Ok(())
                            })
                            .unwrap_or_else(|| Err(anyhow!("Expected audio for this client")))?;
                    }
                    /*_ if topic.ends_with("/session_end") => {
                        debug!("Received msg from server");
                        let msg: MsgAnswer = decode::from_read(std::io::Cursor::new(pub_msg.payload))?;

                    }*/
                    _ => {}
                }
            }
            Event::Incoming(_) => {}
            Event::Outgoing(_) => {}
        }
    }
}

async fn user_listen(
    mqtt_name: &str,
    rec_dev: Rc<AsyncMutex<RecDevice>>,
    mut config: watch::Receiver<ClientConf>,
    client: Rc<RefCell<AsyncClient>>,
) -> anyhow::Result<()> {
    let snowboy_path = SNOWBOY_DATA_PATH.resolve();
    let mut pas_listener = {
        let hotword_det = Snowboy::new(
            &snowboy_path.join("lily.pmdl"),
            &snowboy_path.join("common.res"),
            config.borrow().hotword_sensitivity,
        )?;
        PasiveListener::new(hotword_det)?
    };

    let vad = SnowboyVad::new(&snowboy_path.join("common.res"))?;
    let mut act_listener = ActiveListener::new(vad);
    let mut current_state = ProgState::PasiveListening;

    let mut debugaudio = DebugAudio::new(2000);
    rec_dev
        .lock()
        .await
        .start_recording()
        .expect(AUDIO_REC_START_ERR_MSG);
    let mut activeaudio = AudioRaw::new_empty(DEFAULT_SAMPLES_PER_SECOND);
    let mut activeaudio_raw = AudioRaw::new_empty(DEFAULT_SAMPLES_PER_SECOND);
    fn wait_key(key_channel_in: mpsc::Sender<()>, key: Key) {
        let stdin = stdin();
        let stdin = stdin.lock();
        let mut input = stdin.keys();
        loop {
            if input.next().map_or(false, |r|r.map_or(false,|k|{k == key})) {
                key_channel_in.blocking_send(()).map_err(|e|println!("{}", e)).unwrap();
                break;
            }
        }
    }

    let (key_channel_in, mut key_channel_out) = mpsc::channel::<()>(1);
    let _waiter_thread = std::thread::spawn(move || {wait_key(key_channel_in, Key::Char('l'))});
    
    loop {
        let interval = if current_state == ProgState::PasiveListening {
            HOTWORD_CHECK_INTERVAL_MS
        } else {
            ACTIVE_LISTENING_INTERVAL_MS
        };
        

        match current_state {
            ProgState::PasiveListening => {
                let mut rec_guard = rec_dev.lock().await;
                tokio::select! {
                    conf = config.changed() => {
                        conf?;
                        pas_listener.set_from_conf(&config.borrow());
                    }
                    r = rec_guard.read_for_ms(interval) => {
                        if let Some(d) = r? {

                            let mic_data = AudioRef::from(d);
                            if cfg!(debug_assertions) {
                                debugaudio.push(&mic_data);
                            }

                            if pas_listener.process(mic_data)? {
                                current_state = ProgState::ActiveListening(rec_guard);

                                debug!("I'm listening for your command");

                                let msg_pack = encode::to_vec(&MsgEvent{satellite: mqtt_name.to_string(), event: "init_reco".into()})?;
                                client.borrow_mut().publish("lily/event", QoS::AtMostOnce, false, msg_pack).await?;
                            }
                        }
                    }

                    _ = key_channel_out.recv() => {
                        println!("Hey2");
                        current_state = ProgState::ActiveListening(rec_guard);

                        debug!("I'm listening for your command");

                        let msg_pack = encode::to_vec(&MsgEvent{satellite: mqtt_name.to_string(), event: "init_reco".into()})?;
                        client.borrow_mut().publish("lily/event", QoS::AtMostOnce, false, msg_pack).await?;
                    }
                }
            }
            ProgState::ActiveListening(ref mut rec_guard) => {
                let mic_data = match rec_guard.read_for_ms(interval).await? {
                    Some(d) => AudioRef::from(d),
                    None => continue,
                };
                let audio = mic_data.clone().into_owned().to_ogg_opus().unwrap();
                let (a2, _,) =
                    opus_decode::<_, DEFAULT_SAMPLES_PER_SECOND>(Cursor::new(audio)).unwrap();
                if cfg!(debug_assertions) {
                    activeaudio
                        .append_audio(&a2, DEFAULT_SAMPLES_PER_SECOND)
                        .unwrap();
                    activeaudio_raw
                        .append_audio(mic_data.data, DEFAULT_SAMPLES_PER_SECOND)
                        .unwrap();
                }
                match act_listener.process(mic_data)? {
                    ActiveState::NoOneTalking => {}
                    ActiveState::Hearing(data) => {
                        send_audio(mqtt_name, client.clone(), data, false).await?
                    }
                    ActiveState::Done(data) => {
                        if cfg!(debug_assertions) {
                            activeaudio
                                .save_to_disk(Path::new("active_audio.ogg"))
                                .unwrap();
                            activeaudio_raw
                                .save_to_disk(Path::new("active_audio_raw.ogg"))
                                .unwrap();
                            activeaudio.clear();
                            activeaudio_raw.clear();
                        }
                        send_audio(mqtt_name, client.clone(), data, true).await?;

                        current_state = ProgState::PasiveListening;
                    }
                }
            }
        }
    }
}

#[derive(Clone, Deserialize, Debug, Default, Serialize)]
struct ConfFile {
    #[serde(default)]
    mqtt: ConnectionConf,
}

impl ConfFile {
    fn load() -> anyhow::Result<ConfFile> {
        let conf_path = CONN_CONF_FILE.get();
        if conf_path.is_file() {
            let conf_file = File::open(conf_path)?;
            Ok(from_reader(BufReader::new(conf_file))?)
        } else {
            Err(anyhow!("Config file not found"))
        }
    }

    fn save(&mut self) -> anyhow::Result<()> {
        let conf_path = CONN_CONF_FILE.save_path();
        let parent = conf_path.parent().unwrap();
        if !parent.exists() {
            if let Err(e) = create_dir_all(parent) {
                if e.kind() != std::io::ErrorKind::AlreadyExists {
                    return Err(e.into());
                }
            }
        }
        let conf_file = File::create(&conf_path)?;
        let writer = BufWriter::new(&conf_file);
        

        Ok(to_writer(writer, &self)?)
    }
}

#[tokio::main(flavor = "current_thread")]
pub async fn main() -> anyhow::Result<()> {
    init_log();
    set_app_name("lily-satellite");

    let config = ConfFile::load().unwrap_or_else(|_| ConfFile::default());
    let mqtt_conn = {
        let mut old_conf = config.clone();
        let conn = ConnectionConfResolved::from(config.mqtt, || {
            format!("lily-satellite-{}", Uuid::new_v4())
        });

        if old_conf.mqtt.name.is_none() {
            old_conf.mqtt.name = Some(conn.name.clone());
            old_conf.save()?;
        }

        conn
    };

    let msg = MsgGoodbye {
        satellite: mqtt_conn.name.clone(),
    };
    let last_will = LastWill::new(
        "lily/disconnected",
        encode::to_vec(&msg)?,
        QoS::ExactlyOnce,
        false,
    );
    let (client, mut eloop) = make_mqtt_conn(&mqtt_conn, Some(last_will))?;

    client
        .subscribe(&format!("lily/{}/say_msg", mqtt_conn.name), QoS::AtMostOnce)
        .await?;
    client
        .subscribe(&format!("lily/{}/session_end", mqtt_conn.name), QoS::AtMostOnce)
        .await?;
    client
        .subscribe("lily/satellite_welcome", QoS::AtMostOnce)
        .await?;
    let client_share = Rc::new(RefCell::new(client));

    info!("Mqtt connection made");

    let (conf_change_tx, conf_change_rx) = watch::channel(ClientConf::default());
    let rec_dev = Rc::new(AsyncMutex::new(RecDevice::new()));
    try_join!(
        receive(
            &mqtt_conn.name,
            rec_dev.clone(),
            conf_change_tx,
            client_share.clone(),
            &mut eloop
        ),
        user_listen(&mqtt_conn.name, rec_dev, conf_change_rx, client_share,)
    )?;

    Ok(())
}
