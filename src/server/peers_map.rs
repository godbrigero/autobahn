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
