use std::sync::Arc;

use log::{debug, warn};

use crate::{
  message::ServerStateMessage,
  server::{peer::Peer, Server},
};

impl Server {
  pub async fn handle_server_state(self: Arc<Self>, server_state_message: ServerStateMessage) {
    debug!(
      "Handling server state message from UUID: {}",
      server_state_message.uuid
    );

    let peer_arc = self.peers_map.get_by_id(&server_state_message.uuid).await;
    if peer_arc.is_none() {
      warn!(
        "Peer not found, dropping server state message. UUID: {}",
        server_state_message.uuid
      );
      return;
    }

    let peer_gotten_from = peer_arc.unwrap();
    peer_gotten_from
      .write()
      .await
      .update_topics(server_state_message.topics);
  }
}
