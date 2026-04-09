// TODO: Mac -> pi DOES NOT WORK ATM at all. Mac CAN find but the pi doesn't see mac's brodcast at all.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use log::{debug, error, info};
use no_incode_comments::external_doc;
use peer::Peer;
use prost::Message;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio_tungstenite::{
  accept_async_with_config, tungstenite::protocol::WebSocketConfig, WebSocketStream,
};
use topics_map::{TopicsMap, WebSocketWrite};
use uuid::Uuid;

use crate::util::low_level::ExpectedTypedBytes;
use crate::{
  message::{
    AbstractMessage, HeartbeatMessage, MessageType, PublishMessage, ServerForwardMessage,
    ServerStateMessage, TopicMessage, UnsubscribeMessage,
  },
  server::{
    peers_map::PeersMap,
    topics_map::Websock,
    util::{optimally_send_peers, send_peers_server_msg},
  },
  util::{
    proto::{build_proto_message, get_message_type},
    ws::get_config,
    Address,
  },
};

mod messages;
mod peer;
mod peers_map;
mod topics_map;
mod util;

const MAX_CONNECTION_ATTEMPTS: u32 = 5;
const SLEEP_TIME: Duration = Duration::from_secs(1);

pub trait ServerMessageHandler<T: Message + Default> {
  fn handle(
    self: Arc<Self>,
    bytes: ExpectedTypedBytes<T>,
    ws_write: Arc<Websock>,
  ) -> impl std::future::Future<Output = Result<(), String>> + Send;
}

#[external_doc(path = "src/docs/server.md", key = "Server")]
pub struct Server {
  address: Address,
  topics_map: Arc<TopicsMap>,
  peers_map: Arc<PeersMap>,
  all_websockets: Arc<RwLock<HashSet<Arc<Websock>>>>,
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
      all_websockets: Arc::new(RwLock::new(HashSet::new())),
      uuid: Uuid::new_v4().to_string(),
    });
  }

  pub async fn add_peer(self: Arc<Self>, peer_addr: Address) {
    self.peers_map.add_peer(Peer::new(peer_addr.clone())).await;
    self.peers_map.ensure_connections().await;
    debug!("Adding peer: {}", peer_addr);
  }

  // TODO: make sure this works and is safe in prod.
  fn connection_ensure_loop(self: Arc<Self>) {
    tokio::spawn(async move {
      loop {
        let failed_servers = self.peers_map.ensure_connections().await;
        info!(
          "Current state of connections: {} peers: {:?}",
          self.peers_map.len().await,
          self.peers_map.get_all_peers().await,
        );

        if !failed_servers.is_empty() {
          error!("Failed to connect to some peers, retrying in 1 seconds...");
          self
            .peers_map
            .check_connection_attempts_and_remove(MAX_CONNECTION_ATTEMPTS)
            .await;
        }

        let ws_heartbeat_bytes = build_proto_message(&HeartbeatMessage {
          message_type: MessageType::Heartbeat as i32,
          uuid: self.uuid.clone(),
          topics: self.topics_map.get_all_topics().await,
        });
        for ws in self.all_websockets.write().await.iter() {
          let _ = ws
            .ws
            .write()
            .await
            .send(tungstenite::Message::Binary(ws_heartbeat_bytes.clone()))
            .await;
        }

        tokio::time::sleep(SLEEP_TIME).await;
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
    write
      .send(tungstenite::Message::Binary(
        self.topics_map.to_proto(self.uuid.clone()).await,
      ))
      .await
      .expect("Error handle_client, write.send()");

    let ws_write = Arc::new(Websock::new(write));
    self.all_websockets.write().await.insert(ws_write.clone());
    while let Some(msg) = read.next().await {
      let message = match msg {
        Ok(tungstenite::Message::Close(_)) => {
          self
            .clone()
            .handle_client_disconnected(ws_write.clone())
            .await;
          None
        }
        Ok(tungstenite::Message::Ping(payload)) => {
          // Reply with Pong so intermediaries/clients don't consider us dead.
          let _ = ws_write
            .ws
            .write()
            .await
            .send(tungstenite::Message::Pong(payload))
            .await;
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
    let server_state_message = self.topics_map.to_proto(self.uuid.clone()).await;
    self.peers_map.send_to_all(server_state_message).await;
  }

  async fn handle_message(
    self: Arc<Self>,
    message_type: MessageType,
    bytes: Bytes,
    ws_write: Arc<Websock>,
  ) {
    debug!("Processing message of type: {:?}", message_type);

    let result = match message_type {
      MessageType::Publish => {
        self
          .clone()
          .handle(ExpectedTypedBytes::<PublishMessage>::from(bytes), ws_write)
          .await
      }
      MessageType::Subscribe => {
        self
          .clone()
          .handle(ExpectedTypedBytes::<TopicMessage>::from(bytes), ws_write)
          .await
      }
      MessageType::Heartbeat => {
        self
          .clone()
          .handle(
            ExpectedTypedBytes::<HeartbeatMessage>::from(bytes),
            ws_write,
          )
          .await
      }
      MessageType::Unsubscribe => {
        self
          .clone()
          .handle(
            ExpectedTypedBytes::<UnsubscribeMessage>::from(bytes),
            ws_write,
          )
          .await
      }
      MessageType::ServerState => {
        self
          .clone()
          .handle(
            ExpectedTypedBytes::<ServerStateMessage>::from(bytes),
            ws_write,
          )
          .await
      }
      MessageType::ServerForward => {
        self
          .clone()
          .handle(
            ExpectedTypedBytes::<ServerForwardMessage>::from(bytes),
            ws_write,
          )
          .await
      }
    };

    if result.is_err() {
      error!("Error handling message: {:?}", result.err());
      return;
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
