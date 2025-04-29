use std::hash::Hash;

use bytes::Bytes;
use futures_util::stream::SplitSink;
use futures_util::stream::SplitStream;
use futures_util::SinkExt;
use futures_util::StreamExt;
use getset::Getters;
use log::{debug, error};
use no_incode_comments::external_doc;
use prost::Message;
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration, Instant};
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use crate::message_v2::ServerAbstractMessage;
use crate::message_v2::ServerInitMessage;
use crate::message_v2::ServerMessageType;
use crate::util::proto::build_proto_message;
use crate::util::Address;

type WebSocket = (
  SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, tungstenite::Message>,
  SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
);

#[derive(Getters)]
#[external_doc(path = "src/docs/peer.md", key = "Peer")]
pub struct Peer {
  websocket: Option<WebSocket>,

  #[get = "pub"]
  server_id: Option<String>,
  #[get = "pub"]
  address: Address,
}

impl Peer {
  pub fn new(addr: Address) -> Self {
    Self {
      websocket: None,
      server_id: None,
      address: addr,
    }
  }

  #[external_doc(path = "src/docs/peer.md", key = "is_connected")]
  pub fn is_connected(&self) -> bool {
    self.websocket.is_some()
  }

  pub async fn ping(&mut self) -> bool {
    if !self.is_connected() {
      return false;
    }

    let (write, _) = self.websocket.as_mut().unwrap();
    if write
      .send(tungstenite::Message::Ping(build_proto_message(
        &ServerAbstractMessage {
          message_type: ServerMessageType::Ping as i32,
        },
      )))
      .await
      .is_err()
    {
      self.websocket = None;
      return false;
    }

    return true;
  }

  #[external_doc(path = "src/docs/peer.md", key = "send")]
  pub async fn send(&mut self, message: Bytes) -> bool {
    if !self.is_connected() {
      return false;
    }

    let (write, _) = self.websocket.as_mut().unwrap();
    if write
      .send(tungstenite::Message::Binary(message))
      .await
      .is_err()
    {
      self.websocket = None;
      return false;
    }

    true
  }

  #[external_doc(path = "src/docs/peer.md", key = "ensure_connection")]
  pub async fn ensure_connection(&mut self) -> bool {
    if self.is_connected() && self.ping().await {
      return true;
    }

    if let Ok((ws_stream, _)) = connect_async(self.address.build_ws_url()).await {
      debug!("WebSocket connection established successfully");
      self.websocket = Some(ws_stream.split());
    } else {
      error!("Failed to connect to WebSocket");
      return false;
    }

    let (_, read) = self.websocket.as_mut().unwrap();

    let server_state = timeout(Duration::from_secs(1), read.next())
      .await
      .unwrap_or_else(|_| {
        error!("Error with server_state where timeout was reached");
        return None;
      })
      .and_then(|msg| {
        debug!("Received server state message");
        msg.ok()
      })
      .and_then(|msg| {
        let data = msg.into_data();
        debug!("Decoding server state message of {} bytes", data.len());
        ServerInitMessage::decode(Bytes::from(data)).ok()
      })
      .expect("Error with server_state");

    if server_state.message_type == ServerMessageType::Init as i32 {
      self.server_id = Some(server_state.uuid);
    } else {
      return false;
    }

    true
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
