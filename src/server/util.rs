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
