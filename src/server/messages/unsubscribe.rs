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
  util::proto::build_server_state_message,
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

    let message =
      build_server_state_message(self.topics_map.get_all_topics().await, self.uuid.clone());

    self
      .peers_map
      .send_to_peers(&unsubscribe_message.topic, message)
      .await;
  }
}
