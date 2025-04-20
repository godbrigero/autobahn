use std::{collections::HashMap, sync::Arc};

use futures_util::stream::SplitSink;
use tokio::{net::TcpStream, sync::Mutex};
use tokio_tungstenite::WebSocketStream;

pub type WebSocketWrite = Arc<Mutex<SplitSink<WebSocketStream<TcpStream>, tungstenite::Message>>>;
pub type TopicMap = HashMap<String, Vec<WebSocketWrite>>;

pub struct TopicsMap {
  topic_map: TopicMap,
}

impl TopicsMap {
  pub fn new() -> Self {
    return Self {
      topic_map: HashMap::new(),
    };
  }

  pub fn get(&mut self) -> &mut TopicMap {
    return &mut self.topic_map;
  }

  pub fn push(&mut self, topic: String, ws: WebSocketWrite) {
    if self.topic_map.get(&topic).is_none() {
      self.topic_map.insert(topic.clone(), Vec::new());
    }
    self.topic_map.get_mut(&topic).unwrap().push(ws);
  }

  pub fn remove_subscriber_from_topic(&mut self, topic: &str, ws: &WebSocketWrite) {
    if let Some(subscribers) = self.topic_map.get_mut(topic) {
      subscribers.retain(|w| !Arc::ptr_eq(w, ws));
    }
  }

  pub fn remove_subscriber(&mut self, ws: &WebSocketWrite) {
    for topic in &mut self.topic_map {
      topic.1.retain(|w| !Arc::ptr_eq(w, ws));
    }

    self.remove_unused_topics();
  }

  fn remove_unused_topics(&mut self) {
    self
      .topic_map
      .retain(|_, subscribers| !subscribers.is_empty());
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use futures::StreamExt;
  
  use tokio::net::TcpListener;

  async fn create_mock_websocket() -> WebSocketWrite {
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
    let client_ws = Arc::new(Mutex::new(write));

    // Wait for the accept task to complete
    accept_task.await.unwrap();

    client_ws
  }

  #[tokio::test]
  async fn test_new_topics_map() {
    let map = TopicsMap::new();
    assert!(map.topic_map.is_empty());
  }

  #[tokio::test]
  async fn test_push_subscriber() {
    let mut map = TopicsMap::new();
    let ws = create_mock_websocket().await;

    map.push("test_topic".to_string(), ws.clone());

    assert!(map.topic_map.contains_key("test_topic"));
    assert_eq!(map.topic_map.get("test_topic").unwrap().len(), 1);
  }

  #[tokio::test]
  async fn test_remove_subscriber_from_topic() {
    let mut map = TopicsMap::new();
    let ws = create_mock_websocket().await;

    map.push("test_topic".to_string(), ws.clone());
    map.remove_subscriber_from_topic("test_topic", &ws);

    assert!(map.topic_map.get("test_topic").unwrap().is_empty());
  }

  #[tokio::test]
  async fn test_remove_subscriber() {
    let mut map = TopicsMap::new();
    let ws1 = create_mock_websocket().await;
    let ws2 = create_mock_websocket().await;

    map.push("topic1".to_string(), ws1.clone());
    map.push("topic1".to_string(), ws2.clone());
    map.push("topic2".to_string(), ws1.clone());

    map.remove_subscriber(&ws1);

    // Check that ws1 was removed from topic1
    if let Some(subscribers) = map.topic_map.get("topic1") {
      assert_eq!(subscribers.len(), 1);
      assert!(subscribers.iter().all(|w| Arc::ptr_eq(w, &ws2)));
    }

    // Check that topic2 was removed since it's now empty
    assert!(!map.topic_map.contains_key("topic2"));
  }

  #[tokio::test]
  async fn test_remove_unused_topics() {
    let mut map = TopicsMap::new();
    let ws = create_mock_websocket().await;

    map.push("topic1".to_string(), ws.clone());
    map.push("topic2".to_string(), ws.clone());

    map.remove_subscriber_from_topic("topic1", &ws);
    map.remove_unused_topics();

    assert!(!map.topic_map.contains_key("topic1"));
    assert!(map.topic_map.contains_key("topic2"));
  }
}
