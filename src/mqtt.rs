use std::sync::{Arc, Mutex};

use crate::{config::Config};
use crate::signals::mqtt::{MqttInterfaceIn, MqttInterfaceOut};
use crate::signals::order::dev_mgmt::SessionManager;
use crate::skills::hermes::{HermesApiIn, HermesApiOut};
use crate::tts::TtsData;

use anyhow::Result;
use lily_common::communication::*;
use rumqttc::{AsyncClient, Event, EventLoop, Packet};
use tokio::{try_join, sync::mpsc};
use unic_langid::LanguageIdentifier;
pub struct MqttApi {
    api_in: MqttApiIn,
    api_out: MqttApiOut
}
impl MqttApi {
    pub fn new(def_lang: LanguageIdentifier) -> Result<Self> {
        Ok(Self {
            api_in: MqttApiIn::new(def_lang),
            api_out: MqttApiOut::new()?
        })
    }
    pub async fn api_loop (
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
        let client = Arc::new(Mutex::new(client_raw));

        let i = self.api_in.handle(eloop, client.clone(), config, channel_nlu, channel_event);
        let o = self.api_out.handle(curr_langs, &config.tts, def_lang, sessions, client);
        try_join!(i, o)?;
                
        Ok(())
    }
}

pub struct MqttApiIn {
    satellite_server_in: MqttInterfaceIn,
    hermes_in: HermesApiIn,
}

impl MqttApiIn {
    fn new(def_lang: LanguageIdentifier) -> Self {
        Self{
            hermes_in: HermesApiIn::new(def_lang),
            satellite_server_in: MqttInterfaceIn::new()
        }
    }
    async fn handle(
        &mut self,
        mut eloop: EventLoop,
        client: Arc<Mutex<AsyncClient>>,
        config: &Config,
        channel_nlu: mpsc::Sender<MsgRequest>,
        channel_event: mpsc::Sender<MsgEvent>,
    ) -> Result<()> {
        
        MqttInterfaceIn::subscribe(&client).await?;
        HermesApiIn::subscribe(&client).await?;

        loop {
            match eloop.poll().await? {
                Event::Incoming(Packet::Publish(pub_msg)) => {
                    match pub_msg.topic.as_str() {
                        // Lily client related
                        "lily/new_satellite" => {
                            self.satellite_server_in.handle_new_sattelite(&pub_msg.payload, config, &client).await?;
                        }
                        "lily/nlu_process" => {
                            self.satellite_server_in.handle_nlu_process(&pub_msg.payload, &channel_nlu).await?;
                        }
                        "lily/event" => {
                            self.satellite_server_in.handle_event(&pub_msg.payload, &channel_event).await?;
                        }
                        "lily/disconnected" => {
                            self.satellite_server_in.handle_disconnected(&pub_msg.payload).await?;
                        }

                        // Hermes related
                        "hermes/tts/say" => {
                            self.hermes_in.handle_tts_say(&pub_msg.payload).await?;
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
}

pub struct MqttApiOut {
    satellite_server_out: MqttInterfaceOut,
    hermes_out: HermesApiOut,
}

impl MqttApiOut {
    fn new() -> Result<Self> {
        Ok(Self{
            hermes_out: HermesApiOut::new()?,
            satellite_server_out: MqttInterfaceOut::new()?
        })
    }
    
    async fn handle(&mut self,
        curr_langs: &Vec<LanguageIdentifier>,
        tts_conf: &TtsData,
        def_lang: Option<&LanguageIdentifier>,
        sessions: Arc<Mutex<SessionManager>>,
        client: Arc<Mutex<AsyncClient>>
    ) -> Result<()> {
        self.satellite_server_out.handle_out(curr_langs, tts_conf, def_lang, sessions, client).await
    }
    
}