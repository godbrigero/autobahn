use std::{collections::HashMap, sync::Arc};

use getset::Getters;
use log::error;
use peer::Peer;
use tokio::{net::TcpListener, sync::Mutex};
use uuid::Uuid;

mod peer;

use crate::util::Address;

#[derive(Getters)]
pub struct GlobalConnectionServer {
  #[getset(get = "pub")]
  self_uuid: String,
  #[getset(get = "pub")]
  self_addr: Address,
  #[getset(get = "pub")]
  others: Vec<Address>,

  peer_id_map: HashMap<String, Arc<Mutex<Peer>>>,
}

impl GlobalConnectionServer {
  pub fn build(self_uuid: String, self_addr: Address, others: Vec<Address>) -> Self {
    let mut peer_id_map = HashMap::new();
    for other in others.clone() {
      peer_id_map.insert(other.to_string(), Arc::new(Mutex::new(Peer::new(other))));
    }

    return Self {
      self_uuid,
      self_addr,
      others,
      peer_id_map,
    };
  }

  fn generate_random_uuid() -> String {
    Uuid::new_v4().to_string()
  }

  async fn connection_ensure_loop(self: Arc<Self>) {
    tokio::spawn(async move {
      loop {
        let self_clone = self.clone();
        let ensure_futures = self
          .peer_id_map
          .values()
          .map(|peer| async move { peer.lock().await.ensure_connection().await })
          .collect::<Vec<_>>();

        let results = futures_util::future::join_all(ensure_futures).await;
        if !results.iter().all(|&success| success) {
          error!("Failed to connect to some peers, retrying in 1 seconds...");
        }

        drop(self_clone);

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
      }
    });
  }
}
