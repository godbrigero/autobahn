use std::{
  collections::{HashMap, HashSet},
  sync::Arc,
  time::Duration,
};

use bytes::Bytes;
use futures_util::stream::SplitSink;
use futures_util::SinkExt;
use tokio::{net::TcpStream, sync::RwLock};
use tokio_tungstenite::WebSocketStream;
use uuid::Uuid;

use crate::{
  message::{MessageType, ServerStateMessage},
  util::proto::build_proto_message,
};

pub struct Websock {
  pub ws: RwLock<WebSocketWrite>,
  pub ws_id: String,
}

impl std::hash::Hash for Websock {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    self.ws_id.hash(state);
  }
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

  pub async fn get_all_unique_websocks(&self) -> Vec<Arc<Websock>> {
    use std::collections::HashSet;
    let topic_map = self.topic_map.read().await;
    let mut unique = HashSet::new();
    let mut result = Vec::new();
    for subs_lock in topic_map.values() {
      let subs = subs_lock.read().await;
      for ws in subs.iter() {
        // Only add Arc<Websock> if its ws_id is unique.
        if unique.insert(ws.ws_id.clone()) {
          result.push(ws.clone());
        }
      }
    }
    result
  }

  pub async fn to_proto(&self, uuid: String) -> Bytes {
    build_proto_message(&ServerStateMessage {
      topics: self.get_all_topics().await,
      message_type: MessageType::ServerState as i32,
      uuid,
    })
  }

  pub async fn get(&self) -> tokio::sync::RwLockReadGuard<'_, TopicMap> {
    self.topic_map.read().await
  }

  pub async fn send_to_topic(&self, topic: &str, message: tungstenite::Message) {
    // IMPORTANT: never hold any TopicsMap locks across `.await`ing on socket I/O.
    // If one subscriber blocks on send (slow client / dead connection), holding a lock here can:
    // - stall all publishes to this topic
    // - prevent subscribe/unsubscribe cleanup (reconnect appears "broken")
    let subscribers_snapshot: Vec<Arc<Websock>> = {
      let topic_map = self.topic_map.read().await;
      let Some(subscribers) = topic_map.get(topic) else {
        return;
      };
      // Ensure the subscribers read-guard is dropped before `topic_map`.
      let snapshot = {
        let subs_guard = subscribers.read().await;
        subs_guard.clone()
      };
      snapshot
    };

    if subscribers_snapshot.is_empty() {
      return;
    }

    let send_futures = subscribers_snapshot.iter().map(|ws| {
      let message = message.clone();
      async move {
        // Bound send time so a single stuck subscriber can't wedge the topic indefinitely.
        match tokio::time::timeout(Duration::from_secs(2), async {
          ws.ws.write().await.send(message).await
        })
        .await
        {
          Ok(Ok(())) => None,
          _ => Some(ws.ws_id.clone()),
        }
      }
    });

    let failed_ids: HashSet<String> = futures_util::future::join_all(send_futures)
      .await
      .into_iter()
      .flatten()
      .collect();

    if failed_ids.is_empty() {
      return;
    }

    // Prune dead/stuck subscribers for this topic.
    let topic_map = self.topic_map.read().await;
    if let Some(subscribers) = topic_map.get(topic) {
      subscribers
        .write()
        .await
        .retain(|w| !failed_ids.contains(&w.ws_id));
    }
    drop(topic_map);

    self.remove_unused_topics().await;
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

    if !self.topic_map.read().await.contains_key(&topic) {
      // todo: verify this logic and see if its correctly done.
      return;
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

  pub async fn update_topics_for_subscriber(&self, ws: Arc<Websock>, new_topics: Vec<String>) {
    use std::collections::HashSet;

    // 1. Collect all topics the provided `ws` is currently subscribed to.
    let mut current_topics = Vec::new();
    {
      let topic_map = self.topic_map.read().await;
      for (topic, subscribers) in topic_map.iter() {
        if subscribers.read().await.iter().any(|w| w.as_ref().eq(&ws)) {
          current_topics.push(topic.clone());
        }
      }
    }

    // 2. Prepare sets for efficient checking of adds/removes.
    let new_topics_set: HashSet<String> = new_topics.iter().cloned().collect();
    let current_topics_set: HashSet<String> = current_topics.iter().cloned().collect();

    // 3. Remove ws from all topics it's not supposed to be in.
    for topic in current_topics
      .iter()
      .filter(|t| !new_topics_set.contains(*t))
    {
      self.remove_subscriber_from_topic(topic, &ws).await;
    }

    // 4. Add ws to all new topics it isn't currently in.
    for topic in new_topics
      .iter()
      .filter(|t| !current_topics_set.contains(*t))
    {
      self.push(topic.clone(), ws.clone()).await;
    }
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

  use futures_util::SinkExt;
  use tokio::net::TcpListener;
  use tokio::sync::{mpsc, oneshot};
  use tokio_tungstenite::tungstenite;

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

  async fn create_mock_websocket_with_receiver(
  ) -> (Arc<Websock>, mpsc::UnboundedReceiver<tungstenite::Message>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let (tx, rx) = mpsc::unbounded_channel::<tungstenite::Message>();
    let (ready_tx, ready_rx) = oneshot::channel::<()>();

    // Accept side reads whatever the returned Websock sends.
    tokio::spawn(async move {
      let (socket, _) = listener.accept().await.unwrap();
      let ws_stream = tokio_tungstenite::accept_async(socket).await.unwrap();
      let (_write, mut read) = ws_stream.split();
      let _ = ready_tx.send(());

      while let Some(Ok(msg)) = read.next().await {
        let _ = tx.send(msg);
      }
    });

    // Connect side: we return the *write* half wrapped in Websock.
    let socket = tokio::net::TcpStream::connect(addr).await.unwrap();
    let ws_stream = tokio_tungstenite::client_async("ws://localhost", socket)
      .await
      .unwrap()
      .0;
    let (write, _read) = ws_stream.split();
    let client_ws = Arc::new(Websock::new(write));

    let _ = ready_rx.await;
    (client_ws, rx)
  }

  async fn create_mock_websocket_with_closed_sink() -> Arc<Websock> {
    let ws = create_mock_websocket().await;
    let _ = ws.ws.write().await.close().await;
    ws
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

  #[tokio::test]
  async fn test_update_topics_for_subscriber_replaces_topic_membership() {
    let map = TopicsMap::new();
    let ws_target = create_mock_websocket().await;
    let ws_other = create_mock_websocket().await;

    map.push("old".to_string(), ws_target.clone()).await;
    map.push("shared".to_string(), ws_target.clone()).await;
    map.push("shared".to_string(), ws_other.clone()).await;

    map
      .update_topics_for_subscriber(
        ws_target.clone(),
        vec!["shared".to_string(), "new".to_string()],
      )
      .await;

    let topic_map = map.topic_map.read().await;

    assert!(
      !topic_map.contains_key("old"),
      "subscriber should be removed from dropped topics"
    );

    let shared = topic_map.get("shared").expect("shared topic missing");
    let shared = shared.read().await;
    assert_eq!(shared.len(), 2);
    assert!(shared.iter().any(|ws| ws.as_ref().eq(&ws_target)));
    assert!(shared.iter().any(|ws| ws.as_ref().eq(&ws_other)));

    let new = topic_map.get("new").expect("new topic missing");
    let new = new.read().await;
    assert_eq!(new.len(), 1);
    assert!(new.iter().any(|ws| ws.as_ref().eq(&ws_target)));
  }

  #[tokio::test]
  async fn test_send_to_topic_prunes_failed_subscriber_but_keeps_working_ones() {
    let map = TopicsMap::new();
    let (ws_ok, mut rx_ok) = create_mock_websocket_with_receiver().await;
    let ws_bad = create_mock_websocket_with_closed_sink().await;

    map.push("topic".to_string(), ws_ok.clone()).await;
    map.push("topic".to_string(), ws_bad.clone()).await;

    map
      .send_to_topic("topic", tungstenite::Message::Binary(vec![1, 2, 3].into()))
      .await;

    // Working subscriber should still receive the message.
    let received = tokio::time::timeout(std::time::Duration::from_secs(1), rx_ok.recv())
      .await
      .unwrap();
    assert!(matches!(received, Some(tungstenite::Message::Binary(_))));

    // Failed subscriber should be pruned.
    let topic_map = map.topic_map.read().await;
    let subs = topic_map.get("topic").unwrap().read().await;
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0].ws_id.as_str(), ws_ok.ws_id.as_str());
  }

  #[tokio::test]
  async fn test_send_to_topic_removes_topic_if_last_subscriber_fails() {
    let map = TopicsMap::new();
    let ws_bad = create_mock_websocket_with_closed_sink().await;

    map.push("topic".to_string(), ws_bad.clone()).await;

    map
      .send_to_topic("topic", tungstenite::Message::Binary(vec![9, 9, 9].into()))
      .await;

    assert!(!map.topic_map.read().await.contains_key("topic"));
  }

  #[tokio::test]
  async fn test_send_to_topic_does_not_wedge_unsubscribe_paths() {
    let map = TopicsMap::new();
    let ws_bad = create_mock_websocket_with_closed_sink().await;
    map.push("topic".to_string(), ws_bad.clone()).await;

    // If send_to_topic held locks across socket I/O, this join could deadlock/hang.
    let remove_fut = tokio::time::timeout(
      std::time::Duration::from_millis(250),
      map.remove_subscriber_from_topic("topic", &ws_bad),
    );
    let send_fut = tokio::time::timeout(
      std::time::Duration::from_secs(3),
      map.send_to_topic("topic", tungstenite::Message::Binary(vec![7].into())),
    );

    let (remove_res, send_res) = tokio::join!(remove_fut, send_fut);
    remove_res.expect("remove_subscriber_from_topic should not be blocked by send_to_topic");
    send_res.expect("send_to_topic should not hang indefinitely");
  }

  #[tokio::test]
  async fn test_send_to_topic_unknown_topic_is_noop() {
    let map = TopicsMap::new();
    tokio::time::timeout(
      std::time::Duration::from_millis(200),
      map.send_to_topic(
        "does-not-exist",
        tungstenite::Message::Binary(vec![1].into()),
      ),
    )
    .await
    .expect("send_to_topic should return quickly for missing topic");
  }

  #[tokio::test]
  async fn test_send_to_topic_timeout_prunes_locked_subscriber() {
    let map = TopicsMap::new();
    let (ws_ok, mut rx_ok) = create_mock_websocket_with_receiver().await;
    let (ws_slow, mut rx_slow) = create_mock_websocket_with_receiver().await;

    map.push("topic".to_string(), ws_ok.clone()).await;
    map.push("topic".to_string(), ws_slow.clone()).await;

    let (locked_tx, locked_rx) = oneshot::channel::<()>();
    tokio::spawn(async move {
      let _guard = ws_slow.ws.write().await;
      let _ = locked_tx.send(());
      tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    });
    let _ = locked_rx.await;

    map
      .send_to_topic("topic", tungstenite::Message::Binary(vec![4, 5, 6].into()))
      .await;

    // Fast subscriber should receive the message.
    let received = tokio::time::timeout(std::time::Duration::from_secs(1), rx_ok.recv())
      .await
      .unwrap();
    assert!(matches!(received, Some(tungstenite::Message::Binary(_))));

    // Slow subscriber should be pruned (send timed out on acquiring write lock / sending).
    let topic_map = map.topic_map.read().await;
    let subs = topic_map.get("topic").unwrap().read().await;
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0].ws_id.as_str(), ws_ok.ws_id.as_str());

    // And the slow receiver should not see anything.
    let slow_res =
      tokio::time::timeout(std::time::Duration::from_millis(200), rx_slow.recv()).await;
    assert!(slow_res.is_err());
  }
}
