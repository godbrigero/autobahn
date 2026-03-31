use std::sync::Arc;

use bytes::Bytes;
use futures::executor::block_on;
use tokio::sync::RwLock;

use crate::{server::peer::Peer, util::Address};

pub struct PeersMap {
  peers_map: RwLock<Vec<Arc<RwLock<Peer>>>>,
}

impl PeersMap {
  pub fn new() -> Self {
    Self {
      peers_map: RwLock::new(Vec::new()),
    }
  }

  pub async fn get_all_peers(&self) -> Vec<Arc<RwLock<Peer>>> {
    self.peers_map.read().await.clone()
  }

  pub fn new_with_peers(peers: Vec<Peer>) -> Self {
    let mut new_peers = Vec::new();
    for peer in peers {
      new_peers.push(Arc::new(RwLock::new(peer)));
    }

    Self {
      peers_map: RwLock::new(new_peers),
    }
  }

  pub async fn add_peer(&self, peer: Peer) -> bool {
    if self.contains_peer_addr(&peer.address()).await {
      return false;
    }

    self
      .peers_map
      .write()
      .await
      .push(Arc::new(RwLock::new(peer)));

    return true;
  }

  pub async fn update_peers_self_state(&self, message: Bytes) {
    let peers = self.peers_map.read().await;
    for peer in peers.iter() {
      let mut peer_r = peer.write().await;
      peer_r.send_raw(message.clone()).await;
    }
  }

  pub async fn send_to_peers(&self, topic: &str, message: Bytes) {
    let peers = self.peers_map.read().await;
    let mut peers_w_topic = Vec::new();

    for peer in peers.iter() {
      let peer_r = peer.read().await;
      if peer_r.contains_topic(&topic.to_string()) {
        peers_w_topic.push(peer.clone());
      }
    }

    let futures = peers_w_topic.iter().map(|peer| {
      let message = message.clone();
      async move {
        peer.write().await.send_raw(message).await;
      }
    });

    futures_util::future::join_all(futures).await;
  }

  pub async fn contains_peer(&self, server_id: &str) -> bool {
    let peers = self.peers_map.read().await;
    for peer in peers.iter() {
      let peer_r = peer.read().await;
      if peer_r.server_id().unwrap_or_default() == server_id {
        return true;
      }
    }

    return false;
  }

  pub async fn contains_peer_addr(&self, peer_addr: &Address) -> bool {
    let peers = self.peers_map.read().await;
    for peer in peers.iter() {
      let peer_r = peer.read().await;
      if peer_r.address() == *peer_addr {
        return true;
      }
    }

    return false;
  }

  pub async fn ensure_connections(&self) -> Vec<String> {
    let peers = self.peers_map.read().await;
    let mut failed_servers = Vec::new();
    for peer in peers.iter() {
      let mut peer_r = peer.write().await;
      let ok = peer_r.ensure_connection().await;
      if !ok {
        failed_servers.push(peer_r.server_id().unwrap_or_default());
      }
    }

    return failed_servers;
  }

  pub async fn check_connection_attempts_and_remove(&self, max_connection_attempts: u32) {
    let mut peers = self.peers_map.write().await;
    peers.retain(|peer| {
      let peer_r = block_on(peer.read());
      peer_r.get_connection_attempts() <= max_connection_attempts
    });
  }

  pub async fn send_to_all(&self, message: Bytes) {
    let peers = self.peers_map.read().await;
    let futures = peers.iter().map(|peer| {
      let message = message.clone();
      async move {
        let mut peer_r = peer.write().await;
        peer_r.send_raw(message).await;
      }
    });

    futures_util::future::join_all(futures).await;
  }

  pub async fn get_by_id(&self, id: &str) -> Option<Arc<RwLock<Peer>>> {
    let peers = self.peers_map.read().await;
    for peer in peers.iter() {
      let peer_r = peer.read().await;
      if peer_r.server_id() == Some(id.to_string()) {
        return Some(peer.clone());
      }
    }

    return None;
  }

  pub async fn get_by_addr(&self, addr: &Address) -> Option<Arc<RwLock<Peer>>> {
    let peers = self.peers_map.read().await;
    for peer in peers.iter() {
      let peer_r = peer.read().await;
      if peer_r.address() == *addr {
        return Some(peer.clone());
      }
    }

    return None;
  }

  pub async fn len(&self) -> usize {
    let peers = self.peers_map.read().await;
    peers.len()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{
    message::{MessageType, ServerStateMessage},
    util::proto::build_proto_message,
  };
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
  async fn test_add_peer_deduplicates_by_address() {
    let peers = PeersMap::new();
    let addr = Address::from_str("127.0.0.1:12345").unwrap();
    assert!(peers.add_peer(Peer::new(addr.clone())).await);
    assert!(!peers.add_peer(Peer::new(addr)).await);
    assert_eq!(peers.len().await, 1);
  }

  #[tokio::test]
  async fn test_ensure_connections_sets_peer_id_and_topics() {
    let (addr, _rx) = spawn_peer_ws_server("peer-1", vec!["t1".to_string()]).await;
    let peers = PeersMap::new();
    peers.add_peer(Peer::new(addr)).await;

    let failed = peers.ensure_connections().await;
    assert!(failed.is_empty());
    assert!(peers.contains_peer("peer-1").await);

    let peer_arc = peers.get_by_id("peer-1").await.expect("peer-1 missing");
    let peer_r = peer_arc.read().await;
    assert!(peer_r.contains_topic(&"t1".to_string()));
  }

  #[tokio::test]
  async fn test_send_to_peers_filters_by_topic_interest() {
    let (addr_a, mut rx_a) = spawn_peer_ws_server("peer-a", vec!["a".to_string()]).await;
    let (addr_b, mut rx_b) = spawn_peer_ws_server("peer-b", vec!["b".to_string()]).await;

    let peers = PeersMap::new();
    peers.add_peer(Peer::new(addr_a)).await;
    peers.add_peer(Peer::new(addr_b)).await;

    let failed = peers.ensure_connections().await;
    assert!(failed.is_empty());

    let payload = Bytes::from_static(b"hello");
    peers.send_to_peers("a", payload.clone()).await;

    let got_a = tokio::time::timeout(std::time::Duration::from_secs(2), rx_a.recv())
      .await
      .unwrap();
    assert_eq!(got_a.unwrap(), payload);

    let got_b = tokio::time::timeout(std::time::Duration::from_millis(200), rx_b.recv()).await;
    assert!(got_b.is_err(), "peer-b should not receive topic a payload");
  }

  #[tokio::test]
  async fn test_update_peers_self_state_sends_to_all_peers() {
    let (addr_a, mut rx_a) = spawn_peer_ws_server("peer-a2", vec![]).await;
    let (addr_b, mut rx_b) = spawn_peer_ws_server("peer-b2", vec![]).await;

    let peers = PeersMap::new();
    peers.add_peer(Peer::new(addr_a)).await;
    peers.add_peer(Peer::new(addr_b)).await;
    let failed = peers.ensure_connections().await;
    assert!(failed.is_empty());

    let msg = build_proto_message(&ServerStateMessage {
      message_type: MessageType::ServerState as i32,
      uuid: "self".to_string(),
      topics: vec!["x".to_string()],
    });
    peers.update_peers_self_state(msg.clone()).await;

    let got_a = tokio::time::timeout(std::time::Duration::from_secs(2), rx_a.recv())
      .await
      .unwrap()
      .unwrap();
    let got_b = tokio::time::timeout(std::time::Duration::from_secs(2), rx_b.recv())
      .await
      .unwrap()
      .unwrap();
    assert_eq!(got_a, msg);
    assert_eq!(got_b, msg);
  }

  #[tokio::test]
  async fn test_check_connection_attempts_and_remove_drops_failing_peers() {
    let port = free_unused_port();
    let addr = Address::from_str(&format!("127.0.0.1:{}", port)).unwrap();
    let mut peer = Peer::new(addr);

    // Connection should fail quickly (no listener); attempts should increase.
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), peer.ensure_connection())
      .await
      .unwrap();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), peer.ensure_connection())
      .await
      .unwrap();
    assert!(peer.get_connection_attempts() >= 2);

    let peers = PeersMap::new();
    peers.add_peer(peer).await;
    peers.check_connection_attempts_and_remove(1).await;
    assert_eq!(peers.len().await, 0);
  }
}
