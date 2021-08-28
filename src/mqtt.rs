use std::sync::{Arc, Mutex};

use crate::{config::Config};
use crate::signals::mqtt::{MqttInterfaceIn, MqttInterfaceOut};
use crate::signals::order::dev_mgmt::SessionManager;

use anyhow::Result;
use lily_common::communication::*;
use rumqttc::{AsyncClient, Event, EventLoop, Packet, QoS};
use tokio::{try_join, sync::mpsc};
use unic_langid::LanguageIdentifier;

pub struct MqttApi {
    satellite_server_in: MqttInterfaceIn,
    satellite_server_out: MqttInterfaceOut
}
impl MqttApi {
    pub fn new() -> Result<Self> {
        Ok(Self {
            satellite_server_in: MqttInterfaceIn::new(),
            satellite_server_out: MqttInterfaceOut::new()?
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
        client_raw.subscribe("lily/new_satellite", QoS::AtMostOnce).await?;
        client_raw.subscribe("lily/nlu_process", QoS::AtMostOnce).await?;
        client_raw.subscribe("lily/event", QoS::AtMostOnce).await?;
        client_raw.subscribe("lily/disconnected", QoS::ExactlyOnce).await?;
        let client = Arc::new(Mutex::new(client_raw));

        let i = Self::handle_in(&mut self.satellite_server_in, eloop, client.clone(), config, channel_nlu, channel_event);
        let o = self.satellite_server_out.handle_out(curr_langs, &config.tts, def_lang, sessions, client);
        try_join!(i, o)?;
                
        Ok(())
    }

    
    async fn handle_in(
        satellite_server_in: &mut MqttInterfaceIn,
        mut eloop: EventLoop,
        client: Arc<Mutex<AsyncClient>>,
        config: &Config,
        channel_nlu: mpsc::Sender<MsgRequest>,
        channel_event: mpsc::Sender<MsgEvent>,
    ) -> Result<()> {
        loop {
            match eloop.poll().await? {
                Event::Incoming(Packet::Publish(pub_msg)) => {
                    match pub_msg.topic.as_str() {
                        "lily/new_satellite" => {
                            satellite_server_in.handle_new_sattelite(&pub_msg.payload, config, &client).await?;
                        }
                        "lily/nlu_process" => {
                            satellite_server_in.handle_nlu_process(&pub_msg.payload, &channel_nlu).await?;
                        }
                        "lily/event" => {
                            satellite_server_in.handle_event(&pub_msg.payload, &channel_event).await?;
                        }
                        "lily/disconnected" => {
                            satellite_server_in.handle_disconnected(&pub_msg.payload).await?;
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
}