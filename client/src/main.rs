use rumqttc::{Event, MqttOptions, Client, Packet, QoS};
use url::Url;
use rmp_serde::{decode, encode};
use serde::{Deserialize, Serialize};
#[derive(Deserialize)]
struct MsgAnswer {
    data: String
}

#[derive(Serialize)]
struct MsgNlu {
    hypothesis: String
}
fn main() {
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

    loop {
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
