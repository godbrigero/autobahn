use std::sync::Arc;

use bytes::Bytes;
use log::{debug, error};
use prost::Message;

use crate::{
  message::TopicMessage,
  server::{
    optimally_send_peers,
    topics_map::{WebSocketWrite, Websock},
    Server,
  },
};

impl Server {
  pub async fn handle_subscribe(self: Arc<Self>, bytes: Bytes, ws_write: Arc<Websock>) {
    let subscribe_message = match TopicMessage::decode(bytes.clone()) {
      Ok(msg) => {
        debug!("Client subscribing to topic: {}", msg.topic);
        msg
      }
      Err(e) => {
        error!("Failed to decode subscribe message: {}", e);
        return;
      }
    };

    self
      .topics_map
      .push(subscribe_message.topic.clone(), ws_write)
      .await;

    let server_state_message = self.topics_map.to_proto(self.uuid.clone()).await;

    self
      .peers_map
      .send_to_peers(&subscribe_message.topic, server_state_message)
      .await;
  }
}
