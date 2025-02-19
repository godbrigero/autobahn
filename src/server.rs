use std::{collections::HashMap, sync::Arc};

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use log::{debug, error, info};
use peer::Peer;
use prost::Message;
use tokio::{
  net::{TcpListener, TcpStream},
  sync::Mutex,
};
use tokio_tungstenite::{accept_async, WebSocketStream};
use topics_map::{TopicsMap, WebSocketWrite};
use uuid::Uuid;

use crate::{
  message::{
    AbstractMessage, MessageType, PublishMessage, ServerForwardMessage, ServerStateMessage,
    TopicMessage, UnsubscribeMessage,
  },
  util::{
    proto::{build_proto_message, build_server_state_message, deserialize_partial},
    Address,
  },
};

mod peer;
mod topics_map;

pub struct Server {
  listener: Option<TcpListener>,
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
      listener: None,
      address: self_addr,
      topics_map: Arc::new(Mutex::new(TopicsMap::new())),
      peers_map: Arc::new(Mutex::new(peers)),
      uuid: Uuid::new_v4().to_string(),
    };
  }

  fn connection_ensure_loop(peers_map: Arc<Mutex<HashMap<Address, Peer>>>) {
    tokio::spawn(async move {
      loop {
        let mut peers = peers_map.lock().await;
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

  pub async fn start(&mut self) {
    self.listener = Some(
      TcpListener::bind(format!("0.0.0.0:{}", self.address.port))
        .await
        .expect("Failed to bind server"),
    );

    info!("Server started on {}", self.address.build_ws_url());
    debug!("Server UUID: {}", self.uuid);

    Server::connection_ensure_loop(self.peers_map.clone());

    while let Ok((stream, addr)) = self.listener.as_mut().unwrap().accept().await {
      debug!("New client connection from {}", addr);
      let topics_map = self.topics_map.clone();
      let peers = self.peers_map.clone();
      let uuid = self.uuid.clone();
      tokio::spawn(async move {
        match accept_async(stream).await {
          Ok(ws_stream) => {
            Server::handle_client(uuid, ws_stream, topics_map, peers).await;
          }
          Err(e) => {
            println!("Error in accept_async: {}", e)
          }
        };
      });
    }
  }

  async fn handle_client(
    self_uuid: String,
    ws_stream: WebSocketStream<TcpStream>,
    topics: Arc<Mutex<TopicsMap>>,
    peers: Arc<Mutex<HashMap<Address, Peer>>>,
  ) {
    debug!("Handling new client connection");
    let (mut write, mut read) = ws_stream.split();
    write
      .send(tungstenite::Message::Binary(build_server_state_message(
        topics.lock().await.get().keys().cloned().collect(),
        self_uuid.clone(),
      )))
      .await
      .expect("Error handle_client, write.send()");

    let ws_write = Arc::new(Mutex::new(write));
    while let Some(msg) = read.next().await {
      let message = match msg {
        Ok(tungstenite::Message::Close(_)) => {
          debug!("Client sent close message");
          None
        }
        Ok(tungstenite::Message::Ping(_)) => {
          debug!("Received ping message");
          None
        }
        Ok(msg) => Some(msg),
        Err(e) => {
          error!("Client disconnected with error: {}", e);
          Server::handle_client_disconnected(
            self_uuid.clone(),
            ws_write.clone(),
            topics.clone(),
            peers.clone(),
          )
          .await;
          None
        }
      };

      if let Some(message) = message {
        debug!("Processing message of size {} bytes", message.len());
        let bytes = message.into_data();
        let abstract_message = deserialize_partial::<AbstractMessage>(&bytes);
        if let Ok(abstract_message) = abstract_message {
          debug!(
            "Handling message type: {:?}",
            MessageType::try_from(abstract_message.message_type)
          );
          Server::handle_message(
            MessageType::try_from(abstract_message.message_type).expect("handle_message, try_from"),
            bytes,
            ws_write.clone(),
            topics.clone(),
            peers.clone(),
            self_uuid.clone(),
          )
          .await;
        } else {
          error!("Failed to deserialize message");
        }
      }
    }
  }

  async fn handle_client_disconnected(
    uuid: String,
    ws_write: WebSocketWrite,
    topics_map: Arc<Mutex<TopicsMap>>,
    peers: Arc<Mutex<HashMap<Address, Peer>>>,
  ) {
    debug!("Handling client disconnection");
    topics_map.lock().await.remove_subscriber(&ws_write);
    let server_state_message = build_server_state_message(
      topics_map.lock().await.get().keys().cloned().collect(),
      uuid,
    );

    Server::send_peers_server_msg(
      server_state_message,
      peers.lock().await.values_mut().collect(),
    )
    .await;
  }

  async fn handle_message(
    message_type: MessageType,
    bytes: Bytes,
    ws_write: WebSocketWrite,
    topics_map: Arc<Mutex<TopicsMap>>,
    peers: Arc<Mutex<HashMap<Address, Peer>>>,
    uuid: String,
  ) {
    debug!("Processing message of type: {:?}", message_type);
    match message_type {
      MessageType::ServerState => Server::handle_server_state(
        ServerStateMessage::decode(bytes).expect("Failed decoding MessageType::ServerState"),
        peers.lock().await.values_mut().collect(),
      ),
      MessageType::ServerForward => {
        Server::handle_server_forward(
          ServerForwardMessage::decode(bytes).expect("Failed to decode MessageType::ServerForward"),
          topics_map,
        )
        .await
      }
      MessageType::Publish => Server::handle_publish(bytes, topics_map, peers).await,
      MessageType::Subscribe => {
        Server::handle_subscribe(bytes, ws_write, topics_map, peers, uuid).await
      }
      MessageType::Unsubscribe => {
        Server::handle_unsubscribe(bytes, ws_write, topics_map, peers, uuid).await
      }
      MessageType::ServerStats => {
        Server::handle_server_stats(bytes, ws_write, topics_map, peers, uuid).await
      }
    }
  }

  async fn handle_server_stats(
    bytes: Bytes,
    ws_write: WebSocketWrite,
    topics_map: Arc<Mutex<TopicsMap>>,
    peers: Arc<Mutex<HashMap<Address, Peer>>>,
    uuid: String,
  ) {
  }

  async fn handle_unsubscribe(
    bytes: Bytes,
    ws_write: WebSocketWrite,
    topics_map: Arc<Mutex<TopicsMap>>,
    peers: Arc<Mutex<HashMap<Address, Peer>>>,
    uuid: String,
  ) {
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
    let mut topics_map = topics_map.lock().await;
    topics_map.remove_subscriber_from_topic(&unsubscribe_message.topic, &ws_write);
    let message = build_server_state_message(topics_map.get().keys().cloned().collect(), uuid);
    drop(topics_map);

    Server::optimally_send_peers(message, peers.lock().await.values_mut().collect()).await;
  }

  async fn handle_subscribe(
    bytes: Bytes,
    ws_write: WebSocketWrite,
    topics_map: Arc<Mutex<TopicsMap>>,
    peers: Arc<Mutex<HashMap<Address, Peer>>>,
    uuid: String,
  ) {
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
    let mut topics_map = topics_map.lock().await;
    topics_map.push(subscribe_message.topic, ws_write);
    let server_state_message =
      build_server_state_message(topics_map.get().keys().cloned().collect(), uuid);
    drop(topics_map);

    Server::optimally_send_peers(
      server_state_message,
      peers.lock().await.values_mut().collect(),
    )
    .await;
  }

  async fn handle_publish(
    bytes: Bytes,
    topics_map: Arc<Mutex<TopicsMap>>,
    peers: Arc<Mutex<HashMap<Address, Peer>>>,
  ) {
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
      if let Some(subscriber_list) = topics_map.lock().await.get().get(&topic_message.topic) {
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

    Server::send_peers_server_msg(
      bytes.clone(),
      peers
        .lock()
        .await
        .values_mut()
        .into_iter()
        .filter(|peer| peer.contains_topic(&topic_message.topic))
        .collect(),
    )
    .await;
  }

  async fn handle_server_forward(
    server_forward_message: ServerForwardMessage,
    topics_map: Arc<Mutex<TopicsMap>>,
  ) {
    debug!("Handling server forward message");
    let bytes = Bytes::from(server_forward_message.payload);
    let abstract_message =
      AbstractMessage::decode(bytes.clone()).expect("Error decoding: abstract_message");

    match MessageType::try_from(abstract_message.message_type).unwrap() {
      MessageType::Publish => {
        let publish_message = PublishMessage::decode(bytes.clone())
          .expect("Error decoding publish message in server_forward");

        let subscribers = topics_map
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

    Server::optimally_send_peers(server_message_raw, peers).await;
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
}
