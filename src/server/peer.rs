use std::{collections::HashSet, hash::Hash};

use bytes::Bytes;
use futures_util::{stream::SplitSink, SinkExt, StreamExt};
use getset::Getters;
use log::{debug, error, info, warn};
use prost::Message;
use tokio::net::TcpStream;
use tokio_tungstenite::{
  connect_async, connect_async_with_config, MaybeTlsStream, WebSocketStream,
};
use tungstenite::protocol::WebSocketConfig;

use crate::{message::ServerStateMessage, util::ws::get_config, util::Address};

pub struct Peer {
  topics: HashSet<String>,
  websocket: Option<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, tungstenite::Message>>,

  server_id: Option<String>,
  address: Address,

  connection_attempts: u32,
}

impl std::fmt::Debug for Peer {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(
      f,
      "Peer(server_id: {}, address: {})",
      self.server_id.as_ref().unwrap_or(&String::default()),
      self.address.to_string()
    )
  }
}

impl PartialEq for Peer {
  fn eq(&self, other: &Self) -> bool {
    self.server_id == other.server_id
  }
}

impl Eq for Peer {}

impl Hash for Peer {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    self.server_id.hash(state);
  }
}

impl Peer {
  pub fn new(addr: Address) -> Self {
    debug!("Creating new peer for address: {}", addr.build_ws_url());
    return Self {
      address: addr,
      topics: HashSet::new(),
      websocket: None,
      server_id: None,
      connection_attempts: 0,
    };
  }

  pub fn is_connected(&self) -> bool {
    self.websocket.is_some()
  }

  pub fn get_connection_attempts(&self) -> u32 {
    self.connection_attempts
  }

  pub fn address(&self) -> Address {
    self.address.clone()
  }

  pub fn server_id(&self) -> Option<String> {
    self.server_id.clone()
  }

  pub fn update_topics(&mut self, new_topics: Vec<String>) {
    debug!("Updating topics for peer. Old topics: {:?}", self.topics);
    self.topics.clear();
    for topic in new_topics.iter() {
      self.topics.insert(topic.clone());
    }
    debug!("New topics set to: {:?}", self.topics);
  }

  pub fn contains_topic(&self, topic: &String) -> bool {
    let contains = self.topics.contains(topic);
    debug!("Checking if peer contains topic '{}': {}", topic, contains);
    return contains;
  }

  /*
  /// For future use
  pub async fn send<T: Message>(&mut self, message: &T) {
    self.send_raw(build_proto_message(message)).await;
  }
   */

  pub async fn send_raw(&mut self, payload: Bytes) -> bool {
    debug!(
      "Sending raw message to peer, payload size: {} bytes",
      payload.len()
    );

    self.ensure_connection().await;
    if self.websocket.is_none() {
      warn!(
        "Dropped payload of size {} bytes because can't send to other server. (send_raw())",
        payload.len()
      );
      return false;
    }

    match self
      .websocket
      .as_mut()
      .unwrap()
      .send(tungstenite::Message::Binary(payload))
      .await
    {
      Ok(_) => {
        debug!("Successfully sent message to peer");
        return true;
      }

      Err(e) => {
        self.close_websock().await;
        error!("Failed to send message to peer: {}", e);
        return false;
      }
    }
  }

  pub async fn close_websock(&mut self) {
    match self.websocket.take().unwrap().close().await {
      Ok(_) => debug!("Successfully closed websocket"),
      Err(e) => error!("Failed to close websocket: {}", e),
    }
  }

  pub async fn ensure_connection(&mut self) -> bool {
    if self.websocket.is_some() {
      debug!("Connection already established");
      return true;
    }

    info!(
      "Trying to establish a new connection to {}",
      self.address.build_ws_url()
    );
    self.connection_attempts += 1;
    let config = get_config();

    let req_result =
      match connect_async_with_config(self.address.build_ws_url(), Some(config), false).await {
        Ok((ws_stream, _)) => {
          debug!("WebSocket connection established successfully");
          Some(ws_stream.split())
        }
        Err(e) => {
          error!("Failed to connect to WebSocket: {}", e);
          None
        }
      };

    if req_result.is_none() {
      return false;
    }

    let (sink, mut stream) = req_result.unwrap();
    self.websocket = Some(sink);

    debug!("Waiting for initial server state message");
    let server_state = stream
      .next()
      .await
      .and_then(|msg| {
        debug!("Received server state message");
        msg.ok()
      })
      .and_then(|msg| {
        let data = msg.into_data();
        debug!("Decoding server state message of {} bytes", data.len());
        ServerStateMessage::decode(Bytes::from(data)).ok()
      })
      .map(|payload| {
        debug!("Server state decoded successfully");
        (payload.topics, payload.uuid)
      })
      .expect("Error with server_state");

    info!("Connected to server with ID: {}", server_state.1);
    debug!("Received topics from server: {:?}", server_state.0);

    server_state.0.iter().for_each(|f| {
      self.topics.insert(f.clone());
    });
    self.server_id = Some(server_state.1);
    debug!("Peer initialization complete");

    self.connection_attempts = 0;
    return true;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::message::{MessageType, ServerStateMessage};
  use crate::util::proto::build_proto_message;
  use bytes::Bytes;
  use futures_util::{SinkExt, StreamExt};
  use tokio::sync::mpsc;

  async fn spawn_peer_ws_server(
    uuid: &str,
    topics: Vec<String>,
  ) -> (Address, mpsc::UnboundedReceiver<Bytes>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
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

  fn free_unused_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
      .unwrap()
      .local_addr()
      .unwrap()
      .port()
  }

  #[tokio::test]
  async fn test_ensure_connection_sets_id_topics_and_resets_attempts() {
    let (addr, _rx) = spawn_peer_ws_server("peer-1", vec!["t".to_string()]).await;
    let mut peer = Peer::new(addr);

    // Force attempts > 0
    peer.connection_attempts = 3;

    assert!(peer.ensure_connection().await);
    assert!(peer.is_connected());
    assert_eq!(peer.server_id(), Some("peer-1".to_string()));
    assert!(peer.contains_topic(&"t".to_string()));
    assert_eq!(peer.get_connection_attempts(), 0);

    // Second call should be fast and not change attempts.
    assert!(peer.ensure_connection().await);
    assert_eq!(peer.get_connection_attempts(), 0);
  }

  #[tokio::test]
  async fn test_send_raw_sends_bytes_to_peer() {
    let (addr, mut rx) = spawn_peer_ws_server("peer-2", vec![]).await;
    let mut peer = Peer::new(addr);
    assert!(peer.ensure_connection().await);

    let payload = Bytes::from_static(b"hello");
    assert!(peer.send_raw(payload.clone()).await);

    let got = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
      .await
      .unwrap()
      .unwrap();
    assert_eq!(got, payload);
  }

  #[tokio::test]
  async fn test_send_raw_to_unreachable_peer_returns_false() {
    let port = free_unused_port();
    let addr = Address::from_str(&format!("127.0.0.1:{}", port)).unwrap();
    let mut peer = Peer::new(addr);

    let payload = Bytes::from_static(b"x");
    let res = tokio::time::timeout(std::time::Duration::from_secs(2), peer.send_raw(payload))
      .await
      .unwrap();
    assert!(!res);
    assert!(!peer.is_connected());
    assert!(peer.get_connection_attempts() >= 1);
  }

  #[tokio::test]
  async fn test_close_websock_disconnects() {
    let (addr, _rx) = spawn_peer_ws_server("peer-3", vec![]).await;
    let mut peer = Peer::new(addr);
    assert!(peer.ensure_connection().await);
    assert!(peer.is_connected());

    peer.close_websock().await;
    assert!(!peer.is_connected());
  }

  #[tokio::test]
  async fn test_update_topics_replaces_set() {
    let port = free_unused_port();
    let addr = Address::from_str(&format!("127.0.0.1:{}", port)).unwrap();
    let mut peer = Peer::new(addr);

    peer.update_topics(vec!["a".to_string(), "b".to_string()]);
    assert!(peer.contains_topic(&"a".to_string()));
    assert!(peer.contains_topic(&"b".to_string()));

    peer.update_topics(vec!["c".to_string()]);
    assert!(!peer.contains_topic(&"a".to_string()));
    assert!(peer.contains_topic(&"c".to_string()));
  }
}
