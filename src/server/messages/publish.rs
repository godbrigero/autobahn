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

    let subscribers = {
      let mut topics_map = self.topics_map.lock().await;
      topics_map
        .get()
        .get(&topic_message.topic)
        .cloned()
        .unwrap_or_else(Vec::new)
    };

    let futures = subscribers.iter().map(|subscriber| {
      let bytes = bytes.clone();
      async move {
        let _ = subscriber
          .lock()
          .await
          .send(tungstenite::Message::Binary(bytes))
          .await;
      }
    });
    futures_util::future::join_all(futures).await;

    send_peers_server_msg(
      bytes.clone(),
      self
        .peers_map
        .lock()
        .await
        .values_mut()
        .into_iter()
        .filter(|peer| peer.contains_topic(&topic_message.topic))
        .collect(),
    )
    .await;
  }
}
