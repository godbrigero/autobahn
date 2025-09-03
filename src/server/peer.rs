use std::{collections::HashSet, hash::Hash};

use bytes::Bytes;
use futures_util::{stream::SplitSink, SinkExt, StreamExt};
use getset::Getters;
use log::{debug, error, info};
use prost::Message;
use tokio::net::TcpStream;
use tokio_tungstenite::{
  connect_async, connect_async_with_config, MaybeTlsStream, WebSocketStream,
};
use tungstenite::protocol::WebSocketConfig;

use crate::{message::ServerStateMessage, util::ws::get_config, util::Address};

#[derive(Getters)]
pub struct Peer {
  topics: HashSet<String>,
  websocket: Option<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, tungstenite::Message>>,

  #[get = "pub"]
  server_id: Option<String>,
  #[get = "pub"]
  address: Address,
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
    };
  }

  pub fn is_connected(&self) -> bool {
    self.websocket.is_some()
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

  pub async fn send_raw(&mut self, payload: Bytes) {
    debug!(
      "Sending raw message to peer, payload size: {} bytes",
      payload.len()
    );
    self.ensure_connection().await;
    assert!(self.websocket.is_some());
    match self
      .websocket
      .as_mut()
      .unwrap()
      .send(tungstenite::Message::Binary(payload))
      .await
    {
      Ok(_) => debug!("Successfully sent message to peer"),

      Err(e) => {
        self.websocket = None;
        error!("Failed to send message to peer: {}", e);
      }
    }
  }

  pub async fn ensure_connection(&mut self) -> bool {
    if self.websocket.is_some() {
      debug!("Connection already established");
      return true;
    }

    info!(
      "Establishing new connection to {}",
      self.address.build_ws_url()
    );
    let mut config = get_config();

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

    return true;
  }
}
