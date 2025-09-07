use std::sync::Arc;

use bytes::Bytes;
use futures_util::SinkExt;
use log::debug;
use prost::Message;

use crate::{
  message::{
    AbstractMessage, MessageType, PublishMessage, ServerForwardMessage, ServerStateMessage,
  },
  server::{peer::Peer, Server},
};

impl Server {
  pub async fn handle_server_forward(
    self: Arc<Self>,
    server_forward_message: ServerForwardMessage,
  ) {
    debug!("Handling server forward message");
    let bytes = Bytes::from(server_forward_message.payload);
    let abstract_message =
      AbstractMessage::decode(bytes.clone()).expect("Error decoding: abstract_message");

    match MessageType::try_from(abstract_message.message_type).unwrap() {
      MessageType::Publish => {
        let publish_message = PublishMessage::decode(bytes.clone())
          .expect("Error decoding publish message in server_forward");

        self
          .topics_map
          .send_to_topic(
            &publish_message.topic,
            tungstenite::Message::Binary(bytes.clone()),
          )
          .await;
      }
      _ => {}
    }
  }
}
