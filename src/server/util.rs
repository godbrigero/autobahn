use bytes::Bytes;

use crate::{
  message::{MessageType, ServerForwardMessage},
  server::peer::Peer,
  util::proto::build_proto_message,
};

pub async fn send_peers_server_msg(data: Bytes, peers: Vec<&mut Peer>) {
  let server_message_raw = build_proto_message(&ServerForwardMessage {
    message_type: MessageType::ServerForward as i32,
    payload: data.to_vec(),
    ..Default::default()
  });

  optimally_send_peers(server_message_raw, peers).await;
}

pub async fn optimally_send_peers(data: Bytes, peers: Vec<&mut Peer>) {
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
  use crate::message::{MessageType, ServerForwardMessage, ServerStateMessage};
  use bytes::Bytes;
  use futures_util::{SinkExt, StreamExt};
  use prost::Message;
  use tokio::sync::mpsc;

  async fn spawn_peer_ws_server(
    uuid: &str,
    topics: Vec<String>,
  ) -> (crate::util::Address, mpsc::UnboundedReceiver<Bytes>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let addr = crate::util::Address::new("127.0.0.1".to_string(), port as i32);

    let (tx, rx) = mpsc::unbounded_channel::<Bytes>();
    let uuid = uuid.to_string();

    tokio::spawn(async move {
      let (stream, _) = listener.accept().await.unwrap();
      let ws_stream = tokio_tungstenite::accept_async(stream).await.unwrap();
      let (mut write, mut read) = ws_stream.split();

      // Send initial server state so Peer::ensure_connection() completes.
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

  #[tokio::test]
  async fn test_optimally_send_peers_sends_to_all() {
    let (addr_a, mut rx_a) = spawn_peer_ws_server("pa", vec![]).await;
    let (addr_b, mut rx_b) = spawn_peer_ws_server("pb", vec![]).await;

    let mut peer_a = Peer::new(addr_a);
    let mut peer_b = Peer::new(addr_b);
    assert!(peer_a.ensure_connection().await);
    assert!(peer_b.ensure_connection().await);

    let payload = Bytes::from_static(b"hi");
    optimally_send_peers(payload.clone(), vec![&mut peer_a, &mut peer_b]).await;

    let got_a = tokio::time::timeout(std::time::Duration::from_secs(2), rx_a.recv())
      .await
      .unwrap()
      .unwrap();
    let got_b = tokio::time::timeout(std::time::Duration::from_secs(2), rx_b.recv())
      .await
      .unwrap()
      .unwrap();
    assert_eq!(got_a, payload);
    assert_eq!(got_b, payload);
  }

  #[tokio::test]
  async fn test_send_peers_server_msg_wraps_in_server_forward_message() {
    let (addr, mut rx) = spawn_peer_ws_server("p", vec![]).await;
    let mut peer = Peer::new(addr);
    assert!(peer.ensure_connection().await);

    let inner = Bytes::from_static(b"inner");
    send_peers_server_msg(inner.clone(), vec![&mut peer]).await;

    let got = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
      .await
      .unwrap()
      .unwrap();
    let decoded = ServerForwardMessage::decode(got).unwrap();
    assert_eq!(decoded.message_type, MessageType::ServerForward as i32);
    assert_eq!(decoded.payload, inner.to_vec());
  }
}
