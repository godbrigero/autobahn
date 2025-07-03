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
  accept_async, accept_async_with_config, tungstenite::protocol::WebSocketConfig, WebSocketStream,
};
use topics_map::{TopicsMap, WebSocketWrite};
use uuid::Uuid;

use crate::{
  message::{
    AbstractMessage, MessageType, PublishMessage, ServerForwardMessage, ServerStateMessage,
    TopicMessage, UnsubscribeMessage,
  },
  util::{
    proto::{
      build_proto_message, build_server_state_message, deserialize_partial, fast_get_message_type,
    },
    Address,
  },
};

mod peer;
mod topics_map;

// Define the WebSocket config constants
const MAX_MESSAGE_SIZE: usize = 50 * 1024 * 1024; // 50MB
const MAX_FRAME_SIZE: usize = 50 * 1024 * 1024; // 50MB

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

  fn connection_ensure_loop(self: Arc<Self>) {
    tokio::spawn(async move {
      loop {
        let mut peers = self.peers_map.lock().await;
        let connect_futures = peers.values_mut().map(|peer| peer.ensure_connection());
        let results = futures_util::future::join_all(connect_futures).await;
        if !results.iter().all(|&success| success) {
          error!("Failed to connect to some peers, retrying in 1 seconds...");
        }

        drop(peers);
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
      std::thread::spawn(move || {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
          let mut config = WebSocketConfig::default();
          config.max_message_size = Some(MAX_MESSAGE_SIZE);
          config.max_frame_size = Some(MAX_FRAME_SIZE);

          match accept_async_with_config(stream, Some(config)).await {
            Ok(ws_stream) => {
              self_clone.handle_client(ws_stream).await;
            }
            Err(e) => {
              println!("Error in accept_async: {}", e)
            }
          };
        });
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
        let msg_type = fast_get_message_type(&bytes);
        if let Some(msg_type) = msg_type {
          debug!("Handling message type: {:?}", msg_type);
          self
            .clone()
            .handle_message(msg_type, bytes, ws_write.clone())
            .await;
        } else {
          error!("Failed to deserialize message");
        }
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
      MessageType::ServerState => handle_server_state(
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

  async fn handle_unsubscribe(self: Arc<Self>, bytes: Bytes, ws_write: WebSocketWrite) {
    let unsubscribe_message = match UnsubscribeMessage::decode(bytes.clone()) {
      Ok(msg) => {
        debug!("Client unsubscribing from topic: {}", msg.topic);
        msg
      }
      Err(e) => {
        error!("Failed to decode unsubscribe message: {}", e);
        return;
      }
    };
    let mut topics_map = self.topics_map.lock().await;
    topics_map.remove_subscriber_from_topic(&unsubscribe_message.topic, &ws_write);
    let message = build_server_state_message(
      topics_map.get().keys().cloned().collect(),
      self.uuid.clone(),
    );
    drop(topics_map);

    optimally_send_peers(message, self.peers_map.lock().await.values_mut().collect()).await;
  }

  async fn handle_subscribe(self: Arc<Self>, bytes: Bytes, ws_write: WebSocketWrite) {
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
    let mut topics_map = self.topics_map.lock().await;
    topics_map.push(subscribe_message.topic, ws_write);
    let server_state_message = build_server_state_message(
      topics_map.get().keys().cloned().collect(),
      self.uuid.clone(),
    );
    drop(topics_map);

    optimally_send_peers(
      server_state_message,
      self.peers_map.lock().await.values_mut().collect(),
    )
    .await;
  }

  async fn handle_publish(self: Arc<Self>, bytes: Bytes) {
    let topic_message = match deserialize_partial::<TopicMessage>(&bytes) {
      Ok(msg) => {
        debug!("Publishing message to topic: {}", msg.topic);
        msg
      }
      Err(e) => {
        error!("Failed to deserialize publish message: {}", e);
        return;
      }
    };

    let send_to_subscribers = async {
      if let Some(subscriber_list) = self.topics_map.lock().await.get().get(&topic_message.topic) {
        let futures = subscriber_list.iter().map(|subscriber| {
          let bytes = bytes.clone();
          async move {
            let _ = subscriber
              .lock()
              .await
              .send(tungstenite::Message::Binary(bytes))
              .await;
          }
        });
        futures_util::future::join_all(futures).await;
      }
    };

    tokio::join!(send_to_subscribers);

    send_peers_server_msg(
      bytes.clone(),
      self
        .peers_map
        .lock()
        .await
        .values_mut()
        .into_iter()
        .filter(|peer| peer.contains_topic(&topic_message.topic))
        .collect(),
    )
    .await;
  }

  async fn handle_server_forward(self: Arc<Self>, server_forward_message: ServerForwardMessage) {
    debug!("Handling server forward message");
    let bytes = Bytes::from(server_forward_message.payload);
    let abstract_message =
      AbstractMessage::decode(bytes.clone()).expect("Error decoding: abstract_message");

    match MessageType::try_from(abstract_message.message_type).unwrap() {
      MessageType::Publish => {
        let publish_message = PublishMessage::decode(bytes.clone())
          .expect("Error decoding publish message in server_forward");

        let subscribers = self
          .topics_map
          .lock()
          .await
          .get()
          .get(&publish_message.topic)
          .cloned()
          .unwrap_or_else(Vec::new);

        debug!(
          "Handling publish message from server forward {}",
          subscribers.len()
        );

        let futures = subscribers.iter().map(|client| {
          let bytes = bytes.clone();
          async move {
            let _ = client
              .lock()
              .await
              .send(tungstenite::Message::Binary(bytes))
              .await;
          }
        });

        futures_util::future::join_all(futures).await;
      }
      _ => {}
    }
  }
}

fn handle_server_state(server_state_message: ServerStateMessage, mut peers: Vec<&mut Peer>) {
  debug!(
    "Handling server state message from UUID: {}",
    server_state_message.uuid
  );

  let peer_gotten_from = peers
    .iter_mut()
    .find(|peer| {
      if let Some(peer_server_id) = peer.server_id() {
        peer_server_id == &server_state_message.uuid
      } else {
        false
      }
    })
    .expect(
      "Peer from which server message was sent from is not a valid peer: handle_server_state",
    );

  peer_gotten_from.update_topics(server_state_message.topics);
}

async fn send_peers_server_msg(data: Bytes, peers: Vec<&mut Peer>) {
  let server_message_raw = build_proto_message(&ServerForwardMessage {
    message_type: MessageType::ServerForward as i32,
    payload: data.to_vec(),
  });

  optimally_send_peers(server_message_raw, peers).await;
}

async fn optimally_send_peers(data: Bytes, peers: Vec<&mut Peer>) {
  let futures = peers.into_iter().map(|peer| {
    let message = data.clone();
    async move {
      peer.send_raw(message).await;
    }
  });

  futures_util::future::join_all(futures).await;
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
