use std::mem::replace;
use std::sync::{Arc, Mutex};
use std::ops::DerefMut;

use crate::interfaces::{SharedOutput, UserInterface, UserInterfaceOutput};
use crate::config::Config;
use crate::signals::SignalEventShared;
use crate::stt::DecodeRes;

use anyhow::Result;
use pyo3::{types::PyDict, Py};
use rmp_serde::{decode, encode};
use rumqttc::{Event, MqttOptions, Client, Packet, QoS};
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Clone, Debug, Deserialize)]
pub struct MqttConfig {
    #[serde(default = "def_broker")]
    broker: String
}

impl Default for MqttConfig {
    fn default() -> Self {
        Self {
            broker: def_broker()
        }
    }
}

fn def_broker() -> String {
    "127.0.0.1".to_owned()
}

pub struct MqttInterface {
    output: Arc<Mutex<MqttInterfaceOutput>>,
    common_out: Arc<Mutex<Vec<String>>>
}

impl MqttInterface {
    pub fn new() -> Self {        
        let common_out = Arc::new(Mutex::new(Vec::new()));
        Self {output: Arc::new(Mutex::new(MqttInterfaceOutput::create(common_out.clone()))), common_out}
    }
}

#[derive(Serialize)]
struct MsgAnswer {
    data: String
}

#[derive(Deserialize)]
struct MsgNlu {
    hypothesis: String
}

impl UserInterface for MqttInterface {
    fn interface_loop<F: FnMut( Option<DecodeRes>, SignalEventShared)->Result<()>> (&mut self, config: &Config, signal_event: SignalEventShared, base_context: &Py<PyDict>, mut callback: F) -> Result<()> { 
        let mqtt_conf = config.mqtt_conf.clone().unwrap_or(MqttConfig::default());
        let url = Url::parse(
            &format!("http://{}", mqtt_conf.broker) // WOn't work without protocol
        ).unwrap();
        let host = url.host_str().unwrap();
        let port: u16 = url.port().unwrap_or(1883);
        let mut mqttoptions = MqttOptions::new("lily-server", host, port);
        // TODO: Set username and passwd
        mqttoptions.set_keep_alive(5);
    
        let (mut client, mut connection) = Client::new(mqttoptions, 10);
        client.subscribe("lily/nlu_process", QoS::AtMostOnce).unwrap();
        
        for notification in connection.iter() {
            
            println!("Notification = {:?}", notification);
            match notification.unwrap() {
                Event::Incoming(inc_msg) => {
                    match inc_msg {
                        Packet::Publish(pub_msg) => {
                            match pub_msg.topic.as_str() {
                                "lily/nlu_process" => {
                                    let msg_nlu: MsgNlu = decode::from_read(std::io::Cursor::new(pub_msg.payload)).unwrap();
                                    let hypothesis = msg_nlu.hypothesis;
                                    callback(Some(DecodeRes{hypothesis}), signal_event.clone())?;                
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
                Event::Outgoing(_) => {}
            }

            {   
                let msg_vec = replace(self.common_out.lock().unwrap().deref_mut(), Vec::new());
                for msg in msg_vec {
                    let msg_pack = encode::to_vec(&MsgAnswer{data: msg}).unwrap(); 
                    client.publish("lily/say_msg", QoS::AtMostOnce, false, msg_pack).unwrap();
                }
            }
        }

        Ok(())
    }

    fn get_output(&self) -> SharedOutput {
        self.output.clone()
    }
}

pub struct MqttInterfaceOutput {
    common_out: Arc<Mutex<Vec<String>>>
}

impl MqttInterfaceOutput {
    fn create(common_out: Arc<Mutex<Vec<String>>>) -> Self {
        Self{common_out}
    }
}

impl UserInterfaceOutput for MqttInterfaceOutput {
    fn answer(&mut self, input: &str) -> Result<()> {
        self.common_out.lock().unwrap().push(input.to_owned());
        Ok(())
    }
}

