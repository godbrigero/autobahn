use std::sync::Arc;

use bytes::Bytes;
use log::{debug, error};
use prost::Message;

use crate::{
  message::{AbstractMessage, MessageType, PublishMessage, ServerForwardMessage},
  server::{topics_map::Websock, Server, ServerMessageHandler},
  util::low_level::ExpectedTypedBytes,
};

impl ServerMessageHandler<ServerForwardMessage> for Server {
  async fn handle(
    self: Arc<Self>,
    bytes: ExpectedTypedBytes<ServerForwardMessage>,
    _ws_write: Arc<Websock>,
  ) -> Result<(), String> {
    debug!("Handling server forward message");
    let server_forward_message = match ServerForwardMessage::decode(bytes.clone()) {
      Ok(msg) => {
        debug!("Server forward message: {:?}", msg);
        msg
      }
      Err(e) => {
        error!("Error decoding server forward message: {}", e);
        return Err(e.to_string());
      }
    };

    let bytes = Bytes::from(server_forward_message.payload);
    let abstract_message = match AbstractMessage::decode(bytes.clone()) {
      Ok(msg) => {
        debug!("Abstract message: {:?}", msg);
        msg
      }
      Err(e) => {
        error!("Error decoding abstract message: {}", e);
        return Err(e.to_string());
      }
    };

    match MessageType::try_from(abstract_message.message_type).unwrap() {
      MessageType::Publish => {
        let publish_message = match PublishMessage::decode(bytes.clone()) {
          Ok(msg) => {
            debug!("Publish message: {:?}", msg);
            msg
          }
          Err(e) => {
            error!("Error decoding publish message: {}", e);
            return Err(e.to_string());
          }
        };

        self
          .topics_map
          .send_to_topic(&publish_message.topic, tungstenite::Message::Binary(bytes))
          .await;
      }
      _ => {}
    }

    Ok(())
  }
}

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

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{
    message::{MessageType, PublishMessage, TopicMessage},
    server::topics_map::Websock,
    util::{low_level::ExpectedTypedBytes, proto::build_proto_message, Address},
  };
  use futures_util::StreamExt;
  use tokio::net::TcpListener;
  use tokio::sync::mpsc;
  use tokio_tungstenite::tungstenite;

  async fn create_ws_with_receiver() -> (Arc<Websock>, mpsc::UnboundedReceiver<tungstenite::Message>)
  {
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
  async fn test_handle_server_forward_publish_delivers_to_local_subscribers() {
    let server = Server::new(Vec::new(), Address::from_str("127.0.0.1:0").unwrap());
    let (ws, mut rx) = create_ws_with_receiver().await;
    server.topics_map.push("t".to_string(), ws).await;

    let publish = PublishMessage {
      message_type: MessageType::Publish as i32,
      topic: "t".to_string(),
      payload: b"fwd".to_vec(),
      ..Default::default()
    };
    let publish_bytes = build_proto_message(&publish);
    let fwd = ServerForwardMessage {
      message_type: MessageType::ServerForward as i32,
      payload: publish_bytes.to_vec(),
      ..Default::default()
    };

    let result = server
      .clone()
      .handle(
        ExpectedTypedBytes::<ServerForwardMessage>::from(build_proto_message(&fwd)),
        create_ws_with_receiver().await.0,
      )
      .await;
    assert!(result.is_ok());

    let msg = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
      .await
      .unwrap()
      .unwrap();
    let tungstenite::Message::Binary(bin) = msg else {
      panic!("expected binary publish");
    };
    let decoded = PublishMessage::decode(Bytes::from(bin)).unwrap();
    assert_eq!(decoded.payload, b"fwd");
  }

  #[tokio::test]
  async fn test_handle_server_forward_non_publish_is_ignored() {
    let server = Server::new(Vec::new(), Address::from_str("127.0.0.1:0").unwrap());
    let (ws, mut rx) = create_ws_with_receiver().await;
    server.topics_map.push("t".to_string(), ws).await;

    let sub = build_proto_message(&TopicMessage {
      message_type: MessageType::Subscribe as i32,
      topic: "t".to_string(),
    });
    let fwd = ServerForwardMessage {
      message_type: MessageType::ServerForward as i32,
      payload: sub.to_vec(),
      ..Default::default()
    };
    let result = server
      .clone()
      .handle(
        ExpectedTypedBytes::<ServerForwardMessage>::from(build_proto_message(&fwd)),
        create_ws_with_receiver().await.0,
      )
      .await;
    assert!(result.is_ok());

    let res = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv()).await;
    assert!(res.is_err());
  }

  #[tokio::test]
  async fn test_handle_server_forward_bad_bytes_returns_err() {
    let server = Server::new(Vec::new(), Address::from_str("127.0.0.1:0").unwrap());

    let result = server
      .clone()
      .handle(
        ExpectedTypedBytes::<ServerForwardMessage>::from(Bytes::from_static(b"bad")),
        create_ws_with_receiver().await.0,
      )
      .await;

    assert!(result.is_err());
  }
}
