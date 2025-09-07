use std::{collections::HashMap, sync::Arc};

use futures_util::stream::SplitSink;
use futures_util::SinkExt;
use tokio::{
  net::TcpStream,
  sync::{Mutex, RwLock},
};
use tokio_tungstenite::WebSocketStream;
use uuid::Uuid;

pub struct Websock {
  pub ws: RwLock<WebSocketWrite>,
  pub ws_id: String,
}

impl Websock {
  pub fn new(ws: WebSocketWrite) -> Self {
    Self {
      ws: RwLock::new(ws),
      ws_id: Uuid::new_v4().to_string(),
    }
  }
}

impl PartialEq for Websock {
  fn eq(&self, other: &Self) -> bool {
    self.ws_id == other.ws_id
  }
}

impl Eq for Websock {}

impl From<WebSocketWrite> for Websock {
  fn from(ws: WebSocketWrite) -> Self {
    Self::new(ws)
  }
}

pub type WebSocketWrite = SplitSink<WebSocketStream<TcpStream>, tungstenite::Message>;
pub type TopicMap = HashMap<String, RwLock<Vec<Arc<Websock>>>>;

pub struct TopicsMap {
  topic_map: RwLock<TopicMap>,
}

impl TopicsMap {
  pub fn new() -> Self {
    return Self {
      topic_map: RwLock::new(HashMap::new()),
    };
  }

  pub async fn get(&self) -> tokio::sync::RwLockReadGuard<'_, TopicMap> {
    self.topic_map.read().await
  }

  pub async fn send_to_topic(&self, topic: &str, message: tungstenite::Message) {
    let topic_map = self.topic_map.read().await;
    let subscribers = topic_map.get(topic);
    if subscribers.is_none() {
      return;
    }

    let subscribers = subscribers.unwrap().read().await;

    let futures = subscribers.iter().map(|ws| {
      let message = message.clone();
      async move {
        let _ = ws.ws.write().await.send(message).await;
      }
    });

    futures_util::future::join_all(futures).await;
  }

  pub async fn get_all_topics(&self) -> Vec<String> {
    let topic_map = self.topic_map.read().await;
    topic_map.keys().cloned().collect()
  }

  pub async fn push(&self, topic: String, ws: Arc<Websock>) {
    let contains = self.contains_topic(&topic).await;

    if !contains {
      self
        .topic_map
        .write()
        .await
        .insert(topic.clone(), RwLock::new(Vec::new()));
    }

    self
      .topic_map
      .write()
      .await
      .get_mut(&topic)
      .unwrap()
      .write()
      .await
      .push(ws);
  }

  pub async fn contains_topic(&self, topic: &str) -> bool {
    let topic_map = self.topic_map.read().await;
    topic_map.contains_key(topic)
  }

  pub async fn remove_subscriber_from_topic(&self, topic: &str, ws: &Websock) {
    let topic_map = self.topic_map.read().await;
    if let Some(subscribers) = topic_map.get(topic) {
      subscribers.write().await.retain(|w| !w.as_ref().eq(ws));
    }

    drop(topic_map);

    self.remove_unused_topics().await;
  }

  pub async fn remove_subscriber(&self, ws: &Websock) {
    let topic_map = self.topic_map.read().await;
    for topic in topic_map.iter() {
      topic.1.write().await.retain(|w| !w.as_ref().eq(ws));
    }

    drop(topic_map);

    self.remove_unused_topics().await;
  }

  pub async fn remove_unused_topics(&self) {
    let mut topic_map = self.topic_map.write().await;
    let mut to_remove = Vec::new();
    for topic in topic_map.iter() {
      if topic.1.read().await.is_empty() {
        to_remove.push(topic.0.clone());
      }
    }

    for topic in to_remove {
      topic_map.remove(&topic);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use futures::StreamExt;

  use tokio::net::TcpListener;

  async fn create_mock_websocket() -> Arc<Websock> {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Spawn a task to accept the connection
    let accept_task = tokio::spawn(async move {
      let (socket, _) = listener.accept().await.unwrap();
      let ws_stream = tokio_tungstenite::accept_async(socket).await.unwrap();
      let (_, read) = ws_stream.split();
      // Keep the read half alive to prevent the connection from closing
      tokio::spawn(async move {
        let _ = read.collect::<Vec<_>>().await;
      });
    });

    // Connect to the listener
    let socket = tokio::net::TcpStream::connect(addr).await.unwrap();
    let ws_stream = tokio_tungstenite::client_async("ws://localhost", socket)
      .await
      .unwrap()
      .0;
    let (write, _) = ws_stream.split();
    let client_ws = Arc::new(Websock::new(write));

    // Wait for the accept task to complete
    accept_task.await.unwrap();

    client_ws
  }

  #[tokio::test]
  async fn test_new_topics_map() {
    let map = TopicsMap::new();
    assert!(map.topic_map.read().await.is_empty());
  }

  #[tokio::test]
  async fn test_push_subscriber() {
    let map = TopicsMap::new();
    let ws = create_mock_websocket().await;

    map.push("test_topic".to_string(), ws).await;

    assert!(map.topic_map.read().await.contains_key("test_topic"));
    assert_eq!(
      map
        .topic_map
        .read()
        .await
        .get("test_topic")
        .unwrap()
        .read()
        .await
        .len(),
      1
    );
  }

  #[tokio::test]
  async fn test_remove_subscriber_from_topic() {
    let map = TopicsMap::new();
    let ws = create_mock_websocket().await;

    map.push("test_topic".to_string(), ws.clone()).await;
    map.remove_subscriber_from_topic("test_topic", &ws).await;

    assert!(!map.topic_map.read().await.contains_key("test_topic"));
  }

  #[tokio::test]
  async fn test_remove_subscriber() {
    let map = TopicsMap::new();
    let ws1 = create_mock_websocket().await;
    let ws2 = create_mock_websocket().await;

    map.push("topic1".to_string(), ws1.clone()).await;
    map.push("topic1".to_string(), ws2.clone()).await;
    map.push("topic2".to_string(), ws1.clone()).await;

    map.remove_subscriber(&ws1.clone()).await;

    // Check that ws1 was removed from topic1
    if let Some(subscribers) = map.topic_map.read().await.get("topic1") {
      assert_eq!(subscribers.read().await.len(), 1);
      assert!(subscribers.read().await.iter().all(|w| w == &ws2));
    }

    // Check that topic2 was removed since it's now empty
    assert!(!map.topic_map.read().await.contains_key("topic2"));
  }

  #[tokio::test]
  async fn test_remove_unused_topics() {
    let map = TopicsMap::new();
    let ws = create_mock_websocket().await;

    map.push("topic1".to_string(), ws.clone()).await;
    map.push("topic2".to_string(), ws.clone()).await;

    map.remove_subscriber_from_topic("topic1", &ws).await;

    assert!(!map.topic_map.read().await.contains_key("topic1"));
    assert!(map.topic_map.read().await.contains_key("topic2"));
  }
}
