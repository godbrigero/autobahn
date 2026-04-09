use std::sync::Arc;

use log::{debug, error, info, warn};
use prost::Message;

use crate::{
  message::HeartbeatMessage,
  server::{topics_map::Websock, ExpectedTypedBytes, Server, ServerMessageHandler},
};

impl ServerMessageHandler<HeartbeatMessage> for Server {
  fn handle(
    self: Arc<Self>,
    bytes: ExpectedTypedBytes<HeartbeatMessage>,
    ws_write: Arc<Websock>,
  ) -> impl std::future::Future<Output = Result<(), String>> + Send {
    async move {
      let heartbeat_message = match HeartbeatMessage::decode(bytes.clone()) {
        Ok(msg) => {
          debug!("Handling heartbeat message");
          msg
        }
        Err(e) => {
          error!("Failed to decode heartbeat message: {}", e);
          return Err(e.to_string());
        }
      };

      if !heartbeat_message.uuid.is_empty() {
        let uuid = heartbeat_message.uuid;
        let peer = self.peers_map.get_by_id(&uuid).await;
        if peer.is_none() {
          warn!("Peer not found, dropping heartbeat message. UUID: {}", uuid);
          return Err(format!(
            "Peer not found, dropping heartbeat message. UUID: {}",
            uuid
          ));
        }

        let peer = peer.unwrap();
        peer.write().await.update_topics(heartbeat_message.topics);
      } else {
        self
          .topics_map
          .update_topics_for_subscriber(ws_write.clone(), heartbeat_message.topics)
          .await;

        info!("Updated topics for subscriber: {}", ws_write.ws_id);
      }

      Ok(())
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{
    message::{MessageType, ServerStateMessage},
    util::{proto::build_proto_message, Address},
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
    let port = listener.local_addr().unwrap().port();
    let addr = Address::new("127.0.0.1".to_string(), port as i32);

    let (tx, rx) = mpsc::unbounded_channel::<Bytes>();
    let uuid = uuid.to_string();

    tokio::spawn(async move {
      let (stream, _) = listener.accept().await.unwrap();
      let ws_stream = tokio_tungstenite::accept_async(stream).await.unwrap();
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

    (addr, rx)
  }

  #[tokio::test]
  async fn test_handle_heartbeat_updates_known_peer_topics() {
    let (peer_addr, _rx) = spawn_peer_ws_server("peer-heartbeat", vec!["old".to_string()]).await;
    let server = Server::new(vec![peer_addr], Address::from_str("127.0.0.1:0").unwrap());
    let ws = create_ws().await;
    assert!(server.peers_map.ensure_connections().await.is_empty());

    let heartbeat = build_proto_message(&HeartbeatMessage {
      message_type: MessageType::Heartbeat as i32,
      uuid: "peer-heartbeat".to_string(),
      topics: vec!["fresh".to_string()],
    });

    let result = server
      .clone()
      .handle(ExpectedTypedBytes::<HeartbeatMessage>::from(heartbeat), ws)
      .await;
    assert!(result.is_ok());

    let peer = server.peers_map.get_by_id("peer-heartbeat").await.unwrap();
    let peer = peer.read().await;
    assert!(peer.contains_topic(&"fresh".to_string()));
    assert!(!peer.contains_topic(&"old".to_string()));
  }

  #[tokio::test]
  async fn test_handle_heartbeat_registers_topics_for_local_socket_without_uuid() {
    let server = Server::new(Vec::new(), Address::from_str("127.0.0.1:0").unwrap());
    let ws = create_ws().await;

    let heartbeat = build_proto_message(&HeartbeatMessage {
      message_type: MessageType::Heartbeat as i32,
      uuid: "".to_string(),
      topics: vec!["alpha".to_string(), "beta".to_string()],
    });

    let result = server
      .clone()
      .handle(ExpectedTypedBytes::<HeartbeatMessage>::from(heartbeat), ws)
      .await;
    assert!(result.is_ok());

    let topic_map = server.topics_map.get().await;
    assert!(topic_map.contains_key("alpha"));
    assert!(topic_map.contains_key("beta"));
    assert_eq!(topic_map.get("alpha").unwrap().read().await.len(), 1);
    assert_eq!(topic_map.get("beta").unwrap().read().await.len(), 1);
  }

  #[tokio::test]
  async fn test_handle_heartbeat_unknown_peer_returns_err() {
    let server = Server::new(Vec::new(), Address::from_str("127.0.0.1:0").unwrap());
    let ws = create_ws().await;

    let heartbeat = build_proto_message(&HeartbeatMessage {
      message_type: MessageType::Heartbeat as i32,
      uuid: "missing".to_string(),
      topics: vec!["topic".to_string()],
    });

    let result = server
      .clone()
      .handle(ExpectedTypedBytes::<HeartbeatMessage>::from(heartbeat), ws)
      .await;

    assert!(result.is_err());
  }

  #[tokio::test]
  async fn test_handle_heartbeat_bad_bytes_returns_err() {
    let server = Server::new(Vec::new(), Address::from_str("127.0.0.1:0").unwrap());

    let result = server
      .clone()
      .handle(
        ExpectedTypedBytes::<HeartbeatMessage>::from(Bytes::from_static(b"bad")),
        create_ws().await,
      )
      .await;

    assert!(result.is_err());
  }
}
