use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use tokio::{net::TcpListener, sync::RwLock};
use uuid::Uuid;

use crate::{util::Address, websocket::OpenedWebsocket};

pub struct ServerContext {
    pub id: String,
    pub topics_listened: HashSet<String>,
    pub topic_ws_inverse_map: HashMap<String, Arc<RwLock<OpenedWebsocket>>>,
}

pub struct Server {
    context: Arc<ServerContext>,
    address: Address,
}

impl Server {
    pub fn new(address: Address) -> Self {
        Self {
            context: Arc::new(ServerContext {
                id: Uuid::new_v4().to_string(),
                topics_listened: HashSet::new(),
                topic_ws_inverse_map: HashMap::new(),
            }),
            address,
        }
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(self.address.build_ws_url()).await?;
        while let Ok((stream, _)) = listener.accept().await {}
        Ok(())
    }
}
