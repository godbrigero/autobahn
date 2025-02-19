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
