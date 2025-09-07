use std::sync::Arc;

use bytes::Bytes;
use futures_util::SinkExt;
use log::{debug, error};
use prost::Message;

use crate::{
  message::TopicMessage,
  server::{send_peers_server_msg, Server},
};

impl Server {
  pub async fn handle_publish(self: Arc<Self>, bytes: Bytes) {
    let topic_message = match TopicMessage::decode(bytes.clone()) {
      Ok(msg) => {
        debug!("Publishing message to topic: {}", msg.topic);
        msg
      }
      Err(e) => {
        error!("Failed to deserialize publish message: {}", e);
        return;
      }
    };

    self
      .topics_map
      .send_to_topic(
        &topic_message.topic,
        tungstenite::Message::Binary(bytes.clone()),
      )
      .await;

    // PEERS

    self
      .peers_map
      .send_to_peers(&topic_message.topic, bytes)
      .await;
  }
}
