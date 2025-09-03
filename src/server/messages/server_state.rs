use log::debug;

use crate::{
  message::ServerStateMessage,
  server::{peer::Peer, Server},
};

impl Server {
  pub fn handle_server_state(server_state_message: ServerStateMessage, mut peers: Vec<&mut Peer>) {
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
}
