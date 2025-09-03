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
  sync::Mutex,
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
  server::util::{optimally_send_peers, send_peers_server_msg},
  util::{
    proto::{build_proto_message, build_server_state_message, get_message_type},
    ws::get_config,
    Address,
  },
};

mod messages;
mod peer;
mod topics_map;
mod util;

#[external_doc(path = "src/docs/server.md", key = "Server")]
pub struct Server {
  listener: Mutex<Option<TcpListener>>,
  address: Address,
  topics_map: Arc<Mutex<TopicsMap>>,
  peers_map: Arc<Mutex<HashMap<Address, Peer>>>,
  uuid: String,
}

impl Server {
  pub fn new(peer_addrs: Vec<Address>, self_addr: Address) -> Self {
    debug!("Creating new server with {} peers", peer_addrs.len());
    let peers = peer_addrs
      .iter()
      .map(|peer_addr| (peer_addr.clone(), Peer::new(peer_addr.clone())))
      .collect::<HashMap<Address, Peer>>();

    return Self {
      listener: Mutex::new(None),
      address: self_addr,
      topics_map: Arc::new(Mutex::new(TopicsMap::new())),
      peers_map: Arc::new(Mutex::new(peers)),
      uuid: Uuid::new_v4().to_string(),
    };
  }

  pub async fn add_peer(self: Arc<Self>, peer_addr: Address) {
    let mut peers = self.peers_map.lock().await;
    debug!("Adding peer: {}", peer_addr);

    if peers.contains_key(&peer_addr) {
      debug!("Peer already exists: {}", peer_addr);
      return;
    }

    peers.insert(peer_addr.clone(), Peer::new(peer_addr));
  }

  // TODO: make sure this works and is safe in prod.
  fn connection_ensure_loop(self: Arc<Self>) {
    tokio::spawn(async move {
      loop {
        // Copy keys to avoid holding the map lock across awaits
        let peer_addresses: Vec<Address> = {
          let peers = self.peers_map.lock().await;
          peers.keys().cloned().collect()
        };

        let mut any_failed = false;
        for addr in peer_addresses {
          // Temporarily remove the peer to avoid holding the lock across await
          let peer = {
            let mut peers = self.peers_map.lock().await;
            peers.remove(&addr)
          };

          if let Some(mut peer_val) = peer {
            let ok = peer_val.ensure_connection().await;
            if !ok {
              any_failed = true;
            }
            // Put the peer back
            let mut peers = self.peers_map.lock().await;
            peers.insert(addr.clone(), peer_val);
          }
        }

        if any_failed {
          error!("Failed to connect to some peers, retrying in 1 seconds...");
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
      }
    });
  }

  pub async fn start(self: Arc<Self>) {
    let mut listener = self.listener.lock().await;
    *listener = Some(
      TcpListener::bind(format!("0.0.0.0:{}", self.address.port))
        .await
        .expect("Failed to bind server"),
    );

    info!("Server started on {}", self.address.build_ws_url());
    debug!("Server UUID: {}", self.uuid);

    self.clone().connection_ensure_loop();

    while let Ok((stream, addr)) = listener.as_mut().unwrap().accept().await {
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
      .send(tungstenite::Message::Binary(build_server_state_message(
        self.topics_map.lock().await.get_all_topics(),
        self.uuid.clone(),
      )))
      .await
      .expect("Error handle_client, write.send()");

    let ws_write = Arc::new(Mutex::new(write));
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

  async fn handle_client_disconnected(self: Arc<Self>, ws_write: WebSocketWrite) {
    debug!("Handling client disconnection");
    let mut topics_map = self.topics_map.lock().await;
    topics_map.remove_subscriber(&ws_write);
    let server_state_message = build_server_state_message(
      topics_map.get().keys().cloned().collect(),
      self.uuid.clone(),
    );
    drop(topics_map);

    send_peers_server_msg(
      server_state_message,
      self.peers_map.lock().await.values_mut().collect(),
    )
    .await;
  }

  async fn handle_message(
    self: Arc<Self>,
    message_type: MessageType,
    bytes: Bytes,
    ws_write: WebSocketWrite,
  ) {
    debug!("Processing message of type: {:?}", message_type);
    match message_type {
      MessageType::ServerState => Self::handle_server_state(
        ServerStateMessage::decode(bytes).expect("Failed decoding MessageType::ServerState"),
        self.peers_map.lock().await.values_mut().collect(),
      ),
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
    assert_eq!(server.peers_map.lock().await.len(), 1);
    assert!(server.topics_map.lock().await.get().is_empty());
  }

  #[tokio::test]
  async fn test_peer_connection() {
    let self_addr = Address::from_str("127.0.0.1:8080").unwrap();
    let peer_addrs = vec![Address::from_str("127.0.0.1:8081").unwrap()];

    let peer_addr = peer_addrs[0].clone();
    let server = Server::new(peer_addrs.clone(), self_addr.clone());
    let mut peer_map = server.peers_map.lock().await;
    let peer = peer_map.get_mut(&peer_addr).unwrap();
    assert!(!peer.is_connected());
  }

  // Add more test functions for other server functionality
}
