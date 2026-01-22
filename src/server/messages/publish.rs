use std::sync::Arc;

use bytes::Bytes;
use log::{debug, error};
use prost::Message;
use crate::{
  message::{MessageType, PublishMessage, ServerForwardMessage},
  server::Server,
  util::proto::build_proto_message,
};

impl Server {
  pub async fn handle_publish(self: Arc<Self>, bytes: Bytes) {
    let publish_message = match PublishMessage::decode(bytes.clone()) {
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
        &publish_message.topic,
        tungstenite::Message::Binary(bytes.clone()),
      )
      .await;

    // Forward to peers as a ServerForwardMessage envelope.
    let fwd = ServerForwardMessage {
      message_type: MessageType::ServerForward as i32,
      payload: bytes.to_vec(),
    };
    let fwd_bytes = build_proto_message(&fwd);

    self
      .peers_map
      .send_to_peers(&publish_message.topic, fwd_bytes)
      .await;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{
    message::{MessageType, PublishMessage},
    server::topics_map::Websock,
    util::{proto::build_proto_message, Address},
  };
  use futures_util::StreamExt;
  use tokio::net::TcpListener;
  use tokio::sync::mpsc;
  use tokio_tungstenite::tungstenite;

  async fn create_ws_with_receiver() -> (Arc<Websock>, mpsc::UnboundedReceiver<tungstenite::Message>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::unbounded_channel::<tungstenite::Message>();

    tokio::spawn(async move {
      let (socket, _) = listener.accept().await.unwrap();
      let ws_stream = tokio_tungstenite::accept_async(socket).await.unwrap();
      let (_write, mut read) = ws_stream.split();
      while let Some(Ok(msg)) = read.next().await {
        let _ = tx.send(msg);
      }
    });

    let socket = tokio::net::TcpStream::connect(addr).await.unwrap();
    let ws_stream = tokio_tungstenite::client_async("ws://localhost", socket)
      .await
      .unwrap()
      .0;
    let (write, _read) = ws_stream.split();
    (Arc::new(Websock::new(write)), rx)
  }

  #[tokio::test]
  async fn test_handle_publish_delivers_original_publish_bytes() {
    let server = Server::new(Vec::new(), Address::from_str("127.0.0.1:0").unwrap());
    let (ws, mut rx) = create_ws_with_receiver().await;
    server.topics_map.push("t".to_string(), ws).await;

    let publish = PublishMessage {
      message_type: MessageType::Publish as i32,
      topic: "t".to_string(),
      payload: b"payload".to_vec(),
      ..Default::default()
    };
    let bytes = build_proto_message(&publish);

    server.clone().handle_publish(bytes).await;

    let msg = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
      .await
      .unwrap()
      .unwrap();
    let tungstenite::Message::Binary(bin) = msg else {
      panic!("expected binary publish");
    };
    let decoded = PublishMessage::decode(Bytes::from(bin)).unwrap();
    assert_eq!(decoded.topic, "t");
    assert_eq!(decoded.payload, b"payload");
  }

  #[tokio::test]
  async fn test_handle_publish_bad_bytes_is_ignored() {
    let server = Server::new(Vec::new(), Address::from_str("127.0.0.1:0").unwrap());
    let (ws, mut rx) = create_ws_with_receiver().await;
    server.topics_map.push("t".to_string(), ws).await;

    server.clone().handle_publish(Bytes::from_static(b"not-a-proto")).await;

    let res = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv()).await;
    assert!(res.is_err());
  }
}
