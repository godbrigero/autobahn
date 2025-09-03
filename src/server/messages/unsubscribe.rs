use std::sync::Arc;

use bytes::Bytes;
use log::{debug, error};
use prost::Message;

use crate::{
  message::UnsubscribeMessage,
  server::{topics_map::WebSocketWrite, util::optimally_send_peers, Server},
  util::proto::build_server_state_message,
};

impl Server {
  pub async fn handle_unsubscribe(self: Arc<Self>, bytes: Bytes, ws_write: WebSocketWrite) {
    let unsubscribe_message = match UnsubscribeMessage::decode(bytes.clone()) {
      Ok(msg) => {
        debug!("Client unsubscribing from topic: {}", msg.topic);
        msg
      }
      Err(e) => {
        error!("Failed to decode unsubscribe message: {}", e);
        return;
      }
    };
    let mut topics_map = self.topics_map.lock().await;
    topics_map.remove_subscriber_from_topic(&unsubscribe_message.topic, &ws_write);
    let message = build_server_state_message(
      topics_map.get().keys().cloned().collect(),
      self.uuid.clone(),
    );
    drop(topics_map);

    optimally_send_peers(message, self.peers_map.lock().await.values_mut().collect()).await;
  }
}
