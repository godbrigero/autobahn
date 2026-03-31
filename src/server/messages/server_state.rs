use std::sync::Arc;

use log::{debug, error, warn};
use prost::Message;

use crate::{
  message::ServerStateMessage,
  server::{topics_map::Websock, Server, ServerMessageHandler},
  util::low_level::ExpectedTypedBytes,
};

impl ServerMessageHandler<ServerStateMessage> for Server {
  async fn handle(
    self: Arc<Self>,
    bytes: ExpectedTypedBytes<ServerStateMessage>,
    _ws_write: Arc<Websock>,
  ) -> Result<(), String> {
    let server_state_message = match ServerStateMessage::decode(bytes.clone()) {
      Ok(msg) => {
        debug!("Server state message: {:?}", msg);
        msg
      }
      Err(e) => {
        error!("Failed to decode server state message: {}", e);
        return Err(e.to_string());
      }
    };

    debug!(
      "Handling server state message from UUID: {}",
      server_state_message.uuid
    );

    let peer_arc = self.peers_map.get_by_id(&server_state_message.uuid).await;
    if peer_arc.is_none() {
      warn!(
        "Peer not found, dropping server state message. UUID: {}",
        server_state_message.uuid
      );
      return Err(format!(
        "Peer not found, dropping server state message. UUID: {}",
        server_state_message.uuid
      ));
    }

    let peer_gotten_from = peer_arc.unwrap();
    peer_gotten_from
      .write()
      .await
      .update_topics(server_state_message.topics);

    Ok(())
  }
}

impl Server {
  pub async fn handle_server_state(self: Arc<Self>, server_state_message: ServerStateMessage) {
    debug!(
      "Handling server state message from UUID: {}",
      server_state_message.uuid
    );

    let peer_arc = self.peers_map.get_by_id(&server_state_message.uuid).await;
    if peer_arc.is_none() {
      warn!(
        "Peer not found, dropping server state message. UUID: {}",
        server_state_message.uuid
      );
      return;
    }

    let peer_gotten_from = peer_arc.unwrap();
    peer_gotten_from
      .write()
      .await
      .update_topics(server_state_message.topics);
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{
    message::MessageType,
    util::{low_level::ExpectedTypedBytes, proto::build_proto_message},
  };
  use bytes::Bytes;
  use futures_util::{SinkExt, StreamExt};
  use tokio::net::TcpListener;
  use tokio::sync::mpsc;

  async fn spawn_peer_ws_server(
    uuid: &str,
    topics: Vec<String>,
  ) -> (crate::util::Address, mpsc::UnboundedReceiver<Bytes>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let addr = crate::util::Address::new("127.0.0.1".to_string(), port as i32);

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

  #[tokio::test]
  async fn test_handle_server_state_updates_peer_topics() {
    let (peer_addr, _rx) = spawn_peer_ws_server("peer-x", vec!["old".to_string()]).await;
    let server = Server::new(
      vec![peer_addr],
      crate::util::Address::from_str("127.0.0.1:0").unwrap(),
    );
    let ws = create_ws().await;

    // Establish connection so the peer gets a server_id and can be looked up by UUID.
    let _ = server.peers_map.ensure_connections().await;
    assert!(server.peers_map.contains_peer("peer-x").await);

    let result = server
      .clone()
      .handle(
        ExpectedTypedBytes::<ServerStateMessage>::from(build_proto_message(
          &ServerStateMessage {
            message_type: MessageType::ServerState as i32,
            uuid: "peer-x".to_string(),
            topics: vec!["new".to_string()],
          },
        )),
        ws,
      )
      .await;
    assert!(result.is_ok());
    assert!(server.peers_map.contains_peer("peer-x").await);

    let peer_arc = server.peers_map.get_by_id("peer-x").await.unwrap();
    let peer_r = peer_arc.read().await;
    assert!(peer_r.contains_topic(&"new".to_string()));
    assert!(!peer_r.contains_topic(&"old".to_string()));
  }

  #[tokio::test]
  async fn test_handle_server_state_unknown_peer_returns_err() {
    let server = Server::new(
      Vec::new(),
      crate::util::Address::from_str("127.0.0.1:0").unwrap(),
    );
    let ws = create_ws().await;
    let result = server
      .clone()
      .handle(
        ExpectedTypedBytes::<ServerStateMessage>::from(build_proto_message(
          &ServerStateMessage {
            message_type: MessageType::ServerState as i32,
            uuid: "missing".to_string(),
            topics: vec!["t".to_string()],
          },
        )),
        ws,
      )
      .await;
    assert!(result.is_err());
  }

  #[tokio::test]
  async fn test_handle_server_state_bad_bytes_returns_err() {
    let server = Server::new(
      Vec::new(),
      crate::util::Address::from_str("127.0.0.1:0").unwrap(),
    );

    let result = server
      .clone()
      .handle(
        ExpectedTypedBytes::<ServerStateMessage>::from(Bytes::from_static(b"bad")),
        create_ws().await,
      )
      .await;

    assert!(result.is_err());
  }
}
