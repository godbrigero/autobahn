use std::sync::Arc;

use bytes::Bytes;
use log::{debug, error};
use prost::Message;

use crate::{
  message::TopicMessage,
  server::{topics_map::Websock, Server, ServerMessageHandler},
  util::low_level::ExpectedTypedBytes,
};

impl ServerMessageHandler<TopicMessage> for Server {
  async fn handle(
    self: Arc<Self>,
    bytes: ExpectedTypedBytes<TopicMessage>,
    ws_write: Arc<Websock>,
  ) -> Result<(), String> {
    let subscribe_message = match TopicMessage::decode(bytes.clone()) {
      Ok(msg) => {
        debug!("Client subscribing to topic: {}", msg.topic);
        msg
      }
      Err(e) => {
        error!("Failed to decode subscribe message: {}", e);
        return Err(e.to_string());
      }
    };

    self
      .topics_map
      .push(subscribe_message.topic.clone(), ws_write)
      .await;

    let server_state_message = self.topics_map.to_proto(self.uuid.clone()).await;

    self
      .peers_map
      .update_peers_self_state(server_state_message)
      .await;

    Ok(())
  }
}

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
      .update_peers_self_state(server_state_message)
      .await;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{
    message::{MessageType, ServerStateMessage, TopicMessage},
    server::topics_map::Websock,
    util::{low_level::ExpectedTypedBytes, proto::build_proto_message, Address},
  };
  use bytes::Bytes;
  use futures_util::{SinkExt, StreamExt};
  use tokio::net::TcpListener;
  use tokio::sync::mpsc;

  async fn create_ws() -> Arc<Websock> {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
      let (socket, _) = listener.accept().await.unwrap();
      let ws_stream = tokio_tungstenite::accept_async(socket).await.unwrap();
      let (_write, mut read) = ws_stream.split();
      while let Some(_msg) = read.next().await {}
    });

    let socket = tokio::net::TcpStream::connect(addr).await.unwrap();
    let ws_stream = tokio_tungstenite::client_async("ws://localhost", socket)
      .await
      .unwrap()
      .0;
    let (write, _read) = ws_stream.split();
    Arc::new(Websock::new(write))
  }

  async fn spawn_peer_ws_server(
    uuid: &str,
    topics: Vec<String>,
  ) -> (Address, mpsc::UnboundedReceiver<Bytes>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::unbounded_channel::<Bytes>();
    let uuid = uuid.to_string();

    tokio::spawn(async move {
      let (socket, _) = listener.accept().await.unwrap();
      let ws_stream = tokio_tungstenite::accept_async(socket).await.unwrap();
      let (mut write, mut read) = ws_stream.split();

      let server_state = build_proto_message(&ServerStateMessage {
        message_type: MessageType::ServerState as i32,
        uuid,
        topics,
      });
      let _ = write.send(tungstenite::Message::Binary(server_state)).await;

      while let Some(Ok(msg)) = read.next().await {
        let _ = tx.send(Bytes::from(msg.into_data()));
      }
    });

    (Address::new("127.0.0.1".to_string(), addr.port() as i32), rx)
  }

  #[tokio::test]
  async fn test_handle_subscribe_adds_subscriber_to_topic() {
    let server = Server::new(Vec::new(), Address::from_str("127.0.0.1:0").unwrap());
    let ws = create_ws().await;

    let msg = build_proto_message(&TopicMessage {
      message_type: MessageType::Subscribe as i32,
      topic: "topic".to_string(),
    });
    let result = server
      .clone()
      .handle(ExpectedTypedBytes::<TopicMessage>::from(msg), ws.clone())
      .await;
    assert!(result.is_ok());

    let topic_map = server.topics_map.get().await;
    assert!(topic_map.contains_key("topic"));
    assert_eq!(topic_map.get("topic").unwrap().read().await.len(), 1);
  }

  #[tokio::test]
  async fn test_handle_subscribe_updates_peers_with_new_server_state() {
    let (peer_addr, mut rx) =
      spawn_peer_ws_server("peer-subscribe", vec!["old".to_string()]).await;
    let server = Server::new(
      vec![peer_addr],
      Address::from_str("127.0.0.1:0").unwrap(),
    );
    assert!(server.peers_map.ensure_connections().await.is_empty());

    let ws = create_ws().await;
    let msg = build_proto_message(&TopicMessage {
      message_type: MessageType::Subscribe as i32,
      topic: "topic".to_string(),
    });

    let result = server
      .clone()
      .handle(ExpectedTypedBytes::<TopicMessage>::from(msg), ws)
      .await;
    assert!(result.is_ok());

    let forwarded = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
      .await
      .unwrap()
      .unwrap();
    let decoded = ServerStateMessage::decode(forwarded).unwrap();
    assert_eq!(decoded.uuid, server.uuid);
    assert_eq!(decoded.topics, vec!["topic".to_string()]);
  }

  #[tokio::test]
  async fn test_handle_subscribe_bad_bytes_returns_err() {
    let server = Server::new(Vec::new(), Address::from_str("127.0.0.1:0").unwrap());
    let ws = create_ws().await;
    let result = server
      .clone()
      .handle(
        ExpectedTypedBytes::<TopicMessage>::from(Bytes::from_static(b"bad")),
        ws.clone(),
      )
      .await;
    assert!(result.is_err());

    let topic_map = server.topics_map.get().await;
    assert!(topic_map.is_empty());
  }
}
