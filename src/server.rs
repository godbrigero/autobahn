// TODO: Mac -> pi DOES NOT WORK ATM at all. Mac CAN find but the pi doesn't see mac's brodcast at all.

use std::{collections::HashMap, sync::Arc};

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use log::{debug, error, info};
use no_incode_comments::external_doc;
use peer::Peer;
use prost::Message;
use tokio::{
  net::{TcpListener, TcpStream},
  sync::{Mutex, RwLock},
};
use tokio_tungstenite::{
  accept_async_with_config, tungstenite::protocol::WebSocketConfig, WebSocketStream,
};
use topics_map::{TopicsMap, WebSocketWrite};
use uuid::Uuid;

use crate::{
  message::{
    AbstractMessage, MessageType, PublishMessage, ServerForwardMessage, ServerStateMessage,
    TopicMessage, UnsubscribeMessage,
  },
  server::{
    peers_map::PeersMap,
    topics_map::Websock,
    util::{optimally_send_peers, send_peers_server_msg},
  },
  util::{
    proto::{build_proto_message, build_server_state_message, get_message_type},
    ws::get_config,
    Address,
  },
};

mod messages;
mod peer;
mod peers_map;
mod topics_map;
mod util;

#[external_doc(path = "src/docs/server.md", key = "Server")]
pub struct Server {
  address: Address,
  topics_map: Arc<TopicsMap>,
  peers_map: Arc<PeersMap>,
  uuid: String,
}

impl Server {
  pub fn new(peer_addrs: Vec<Address>, self_addr: Address) -> Arc<Self> {
    debug!("Creating new server with {} peers", peer_addrs.len());
    let peers = peer_addrs
      .iter()
      .map(|peer_addr| Peer::new(peer_addr.clone()))
      .collect::<Vec<Peer>>();

    return Arc::new(Self {
      address: self_addr,
      topics_map: Arc::new(TopicsMap::new()),
      peers_map: Arc::new(PeersMap::new_with_peers(peers)),
      uuid: Uuid::new_v4().to_string(),
    });
  }

  pub async fn add_peer(self: Arc<Self>, peer_addr: Address) {
    self.peers_map.add_peer(Peer::new(peer_addr.clone())).await;
    debug!("Adding peer: {}", peer_addr);

    if self.peers_map.contains_peer_addr(&peer_addr).await {
      debug!("Peer already exists: {}", peer_addr);
      return;
    }

    self.peers_map.add_peer(Peer::new(peer_addr)).await;
  }

  // TODO: make sure this works and is safe in prod.
  fn connection_ensure_loop(self: Arc<Self>) {
    tokio::spawn(async move {
      loop {
        let any_failed = self.peers_map.ensure_connections().await;
        if any_failed {
          error!("Failed to connect to some peers, retrying in 1 seconds...");
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
      }
    });
  }

  pub async fn start(self: Arc<Self>) {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", self.address.port))
      .await
      .expect("Failed to bind server");

    info!("Server started on {}", self.address.build_ws_url());
    debug!("Server UUID: {}", self.uuid);

    self.clone().connection_ensure_loop();

    while let Ok((stream, addr)) = listener.accept().await {
      debug!("New client connection from {}", addr);
      let self_clone = self.clone();
      tokio::spawn(async move {
        match accept_async_with_config(stream, Some(get_config())).await {
          Ok(ws_stream) => {
            self_clone.handle_client(ws_stream).await;
          }
          Err(e) => {
            println!("Error in accept_async: {}", e)
          }
        };
      });
    }
  }

  async fn handle_client(self: Arc<Self>, ws_stream: WebSocketStream<TcpStream>) {
    debug!("Handling new client connection");
    let (mut write, mut read) = ws_stream.split();
    let topics = self.topics_map.get_all_topics().await;
    write
      .send(tungstenite::Message::Binary(build_server_state_message(
        topics,
        self.uuid.clone(),
      )))
      .await
      .expect("Error handle_client, write.send()");

    let ws_write = Arc::new(Websock::new(write));
    while let Some(msg) = read.next().await {
      let message = match msg {
        Ok(tungstenite::Message::Close(_)) => {
          self
            .clone()
            .handle_client_disconnected(ws_write.clone())
            .await;
          None
        }
        Ok(tungstenite::Message::Ping(_)) => {
          debug!("Received ping message");
          None
        }
        Ok(msg) => Some(msg),
        Err(e) => {
          error!("Client disconnected with error: {}", e);
          self
            .clone()
            .handle_client_disconnected(ws_write.clone())
            .await;
          None
        }
      };

      if let Some(message) = message {
        debug!("Processing message of size {} bytes", message.len());
        let bytes = message.into_data();
        let msg_type = get_message_type(&bytes);
        self
          .clone()
          .handle_message(msg_type, bytes, ws_write.clone())
          .await;
      }
    }
  }

  async fn handle_client_disconnected(self: Arc<Self>, ws_write: Arc<Websock>) {
    debug!("Handling client disconnection");
    self.topics_map.remove_subscriber(&ws_write).await;
    let topics = self.topics_map.get_all_topics().await;
    let server_state_message = build_server_state_message(topics, self.uuid.clone());

    self.peers_map.send_to_all(server_state_message).await;
  }

  async fn handle_message(
    self: Arc<Self>,
    message_type: MessageType,
    bytes: Bytes,
    ws_write: Arc<Websock>,
  ) {
    debug!("Processing message of type: {:?}", message_type);
    match message_type {
      MessageType::ServerState => {
        self
          .clone()
          .handle_server_state(
            ServerStateMessage::decode(bytes).expect("Failed decoding MessageType::ServerState"),
          )
          .await
      }
      MessageType::ServerForward => {
        self
          .clone()
          .handle_server_forward(
            ServerForwardMessage::decode(bytes)
              .expect("Failed to decode MessageType::ServerForward"),
          )
          .await
      }
      MessageType::Publish => {
        self.clone().handle_publish(bytes).await;
      }
      MessageType::Subscribe => {
        self.clone().handle_subscribe(bytes, ws_write).await;
      }
      MessageType::Unsubscribe => {
        self.clone().handle_unsubscribe(bytes, ws_write).await;
      }
      MessageType::ServerStats => {}
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn test_server_creation() {
    let self_addr = Address::from_str("127.0.0.1:8080").unwrap();
    let peer_addrs = vec![Address::from_str("127.0.0.1:8081").unwrap()];

    let server = Server::new(peer_addrs.clone(), self_addr.clone());

    assert_eq!(server.address, self_addr);
    assert_eq!(server.peers_map.len().await, 1);
    assert!(server.topics_map.get_all_topics().await.is_empty());
  }

  #[tokio::test]
  async fn test_peer_connection() {
    let self_addr = Address::from_str("127.0.0.1:8080").unwrap();
    let peer_addrs = vec![Address::from_str("127.0.0.1:8081").unwrap()];

    let peer_addr = peer_addrs[0].clone();
    let server = Server::new(peer_addrs.clone(), self_addr.clone());
    let peer = server.peers_map.get_by_addr(&peer_addr).await.unwrap();
    assert!(!peer.read().await.is_connected());
  }

  // Add more test functions for other server functionality
}
