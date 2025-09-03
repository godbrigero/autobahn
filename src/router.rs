use std::{collections::HashMap, sync::Arc};

use once_cell::sync::OnceCell;

use async_trait::async_trait;
use prost::Message;
use tokio::sync::RwLock;

use crate::{
    message::{AbstractMessage, MessageType},
    server::ServerContext,
};

static ROUTER: OnceCell<Arc<RwLock<Router>>> = OnceCell::new();

pub fn set_router(router: Arc<RwLock<Router>>) {
    ROUTER.set(router).ok().expect("Router already set");
}

pub fn get_router() -> Arc<RwLock<Router>> {
    ROUTER.get().expect("Router not initialized").clone()
}

pub async fn register_handler<F>(msg_type: MessageType, factory: F)
where
    F: Fn() -> Box<dyn DynHandler> + Send + Sync + 'static,
{
    let router = get_router();
    let mut router = router.write().await;
    router.register_factory(msg_type, factory);
}

#[async_trait]
pub trait DynHandler: Send + Sync {
    async fn handle(&self, raw: &[u8], ctx: Arc<ServerContext>) -> anyhow::Result<()>;
}

pub struct Router {
    handlers: HashMap<i32, Arc<dyn Fn() -> Box<dyn DynHandler> + Send + Sync>>,
}

impl Router {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    pub fn register_factory<F>(&mut self, msg_type: MessageType, factory: F)
    where
        F: Fn() -> Box<dyn DynHandler> + Send + Sync + 'static,
    {
        self.handlers.insert(msg_type as i32, Arc::new(factory));
    }

    pub async fn route(&self, raw: &[u8], ctx: Arc<ServerContext>) -> anyhow::Result<()> {
        let abstract_msg = AbstractMessage::decode(raw)?;
        if let Some(factory) = self.handlers.get(&abstract_msg.message_type) {
            let handler = (factory)();
            handler.handle(raw, ctx).await
        } else {
            Ok(())
        }
    }
}
