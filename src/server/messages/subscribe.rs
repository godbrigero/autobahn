use std::sync::Arc;

use bytes::Bytes;
use log::{debug, error};
use prost::Message;

use crate::{
  message::TopicMessage,
  server::{optimally_send_peers, topics_map::WebSocketWrite, Server},
  util::proto::build_server_state_message,
};

impl Server {
  pub async fn handle_subscribe(self: Arc<Self>, bytes: Bytes, ws_write: WebSocketWrite) {
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
    let mut topics_map = self.topics_map.lock().await;
    topics_map.push(subscribe_message.topic, ws_write);
    let server_state_message = build_server_state_message(
      topics_map.get().keys().cloned().collect(),
      self.uuid.clone(),
    );
    drop(topics_map);

    optimally_send_peers(
      server_state_message,
      self.peers_map.lock().await.values_mut().collect(),
    )
    .await;
  }
}
