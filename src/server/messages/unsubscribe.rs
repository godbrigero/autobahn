use std::sync::Arc;

use bytes::Bytes;
use log::{debug, error};
use prost::Message;

use crate::{
  message::UnsubscribeMessage,
  server::{
    topics_map::{WebSocketWrite, Websock},
    util::optimally_send_peers,
    Server,
  },
};

impl Server {
  pub async fn handle_unsubscribe(self: Arc<Self>, bytes: Bytes, ws_write: Arc<Websock>) {
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
    self
      .topics_map
      .remove_subscriber_from_topic(&unsubscribe_message.topic, &ws_write)
      .await;

    let message = self.topics_map.to_proto(self.uuid.clone()).await;

    self.peers_map.update_peers_self_state(message).await;
  }
}
