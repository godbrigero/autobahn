use autobahn::{
  discovery::Discovery,
  message::{HeartbeatMessage, MessageType, PublishMessage, TopicMessage, UnsubscribeMessage},
  server::Server,
  util::{proto::{build_proto_message, get_message_type}, Address},
};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use prost::Message;
use std::{net::TcpListener, sync::Arc, time::Duration};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::sleep;
use tokio_tungstenite::{connect_async, tungstenite, MaybeTlsStream, WebSocketStream};

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
type WsRead = futures_util::stream::SplitStream<WsStream>;
const HEARTBEAT_PROPAGATION_WAIT: Duration = Duration::from_millis(1500);

fn free_port() -> u16 {
  TcpListener::bind("127.0.0.1:0")
    .unwrap()
    .local_addr()
    .unwrap()
    .port()
}

async fn connect_client(addr: &Address) -> WebSocketStream<MaybeTlsStream<TcpStream>> {
  let url = format!("ws://{}", addr);
  let (ws, _resp) = connect_async(url).await.expect("failed to connect");
  let (write, mut read) = ws.split();

  // Server sends a ServerStateMessage immediately on connect; drain it so tests start cleanly.
  let _ = tokio::time::timeout(Duration::from_secs(1), read.next())
    .await
    .expect("did not receive initial server state")
    .unwrap()
    .unwrap();

  write.reunite(read).expect("failed to reunite stream")
}

async fn recv_publish(read: &mut WsRead, timeout: Duration) -> Option<PublishMessage> {
  let deadline = tokio::time::Instant::now() + timeout;

  loop {
    let remaining = deadline.checked_duration_since(tokio::time::Instant::now())?;
    let msg = tokio::time::timeout(remaining, read.next())
      .await
      .ok()??
      .ok()?;

    match msg {
      tungstenite::Message::Binary(bytes) => {
        let bytes = Bytes::from(bytes);
        match get_message_type(&bytes) {
          MessageType::Publish => return PublishMessage::decode(bytes).ok(),
          MessageType::Heartbeat | MessageType::ServerState => continue,
          other => panic!("unexpected binary message while waiting for publish: {:?}", other),
        }
      }
      tungstenite::Message::Ping(_) | tungstenite::Message::Pong(_) => continue,
      tungstenite::Message::Close(_) => return None,
      _ => continue,
    }
  }
}

fn subscribe_bytes(topic: &str) -> Bytes {
  build_proto_message(&TopicMessage {
    message_type: MessageType::Subscribe as i32,
    topic: topic.to_string(),
  })
}

fn unsubscribe_bytes(topic: &str) -> Bytes {
  build_proto_message(&UnsubscribeMessage {
    message_type: MessageType::Unsubscribe as i32,
    topic: topic.to_string(),
  })
}

fn publish_bytes(topic: &str, payload: &[u8]) -> Bytes {
  build_proto_message(&PublishMessage {
    message_type: MessageType::Publish as i32,
    topic: topic.to_string(),
    payload: payload.to_vec(),
    ..Default::default()
  })
}

fn heartbeat_bytes(topics: &[&str]) -> Bytes {
  build_proto_message(&HeartbeatMessage {
    message_type: MessageType::Heartbeat as i32,
    uuid: None,
    topics: topics.iter().map(|topic| topic.to_string()).collect(),
  })
}

#[tokio::test]
async fn test_server_startup() {
  // AI test
  let self_addr = Address::from_str(&format!("127.0.0.1:{}", free_port())).unwrap();
  let server = Server::new(Vec::new(), self_addr.clone());

  // Start server in a separate task
  let server_handle = tokio::spawn(async move {
    server.start().await;
  });

  // Give server time to start
  tokio::time::sleep(Duration::from_millis(100)).await;

  // Try to connect to the server
  let _ws = connect_client(&self_addr).await;

  server_handle.abort();
}

#[tokio::test]
async fn test_peer_communication() {
  // AI test
  let server1_addr = Address::from_str(&format!("127.0.0.1:{}", free_port())).unwrap();
  let server2_addr = Address::from_str(&format!("127.0.0.1:{}", free_port())).unwrap();

  let server1 = Server::new(vec![server2_addr.clone()], server1_addr.clone());
  let server2 = Server::new(vec![server1_addr.clone()], server2_addr.clone());

  let server1_handle = tokio::spawn(async move {
    server1.start().await;
  });

  let server2_handle = tokio::spawn(async move {
    server2.start().await;
  });

  tokio::time::sleep(Duration::from_millis(1000)).await;

  server1_handle.abort();
  server2_handle.abort();
}

#[tokio::test]
async fn test_server_discovery() {
  // AI test
  let found_ports: Arc<Mutex<Vec<u16>>> = Arc::new(Mutex::new(Vec::new()));
  let port = free_port();
  let discovery_1 = Discovery::new_with_ip("127.0.0.1".to_string(), free_port());
  let discovery_2 = Discovery::new(port);

  let found_ports_clone = found_ports.clone();
  let discovery_1_handle = tokio::spawn(discovery_1.start_discovery_loop(move |_ip, port| {
    let found_ports = found_ports_clone.clone();
    async move {
      let mut found_ports = found_ports.lock().await;
      found_ports.push(port);
    }
  }));

  discovery_2.run_discovery_server();

  sleep(Duration::from_millis(2000)).await;

  discovery_1_handle.abort();

  let found_ports = found_ports.lock().await;
  assert!(found_ports.len() >= 1);
  println!("Found ports: {:?}", found_ports);
  assert!(found_ports.contains(&port));
}

#[tokio::test]
async fn test_single_server_pubsub_delivery() {
  // AI test
  let self_addr = Address::from_str(&format!("127.0.0.1:{}", free_port())).unwrap();
  let server = Server::new(Vec::new(), self_addr.clone());
  let handle = tokio::spawn(async move { server.start().await });

  tokio::time::sleep(Duration::from_millis(150)).await;

  let (mut sub_write, mut sub_read) = connect_client(&self_addr).await.split();
  let (mut pub_write, _pub_read) = connect_client(&self_addr).await.split();

  sub_write
    .send(tungstenite::Message::Binary(subscribe_bytes("t1")))
    .await
    .unwrap();

  tokio::time::sleep(Duration::from_millis(50)).await;

  pub_write
    .send(tungstenite::Message::Binary(publish_bytes("t1", b"hello")))
    .await
    .unwrap();

  // Subscriber should receive the publish, even if heartbeat frames are interleaved.
  let publish = recv_publish(&mut sub_read, Duration::from_secs(2))
    .await
    .expect("expected publish");
  assert_eq!(publish.topic, "t1");
  assert_eq!(publish.payload, b"hello");

  handle.abort();
}

#[tokio::test]
async fn test_unsubscribe_stops_delivery() {
  // AI test
  let self_addr = Address::from_str(&format!("127.0.0.1:{}", free_port())).unwrap();
  let server = Server::new(Vec::new(), self_addr.clone());
  let handle = tokio::spawn(async move { server.start().await });

  tokio::time::sleep(Duration::from_millis(150)).await;

  let (mut sub_write, mut sub_read) = connect_client(&self_addr).await.split();
  let (mut pub_write, _pub_read) = connect_client(&self_addr).await.split();

  sub_write
    .send(tungstenite::Message::Binary(subscribe_bytes("t2")))
    .await
    .unwrap();
  tokio::time::sleep(Duration::from_millis(50)).await;

  sub_write
    .send(tungstenite::Message::Binary(unsubscribe_bytes("t2")))
    .await
    .unwrap();
  tokio::time::sleep(Duration::from_millis(50)).await;

  pub_write
    .send(tungstenite::Message::Binary(publish_bytes("t2", b"nope")))
    .await
    .unwrap();

  // Should not receive anything quickly.
  let res = tokio::time::timeout(Duration::from_millis(300), sub_read.next()).await;
  assert!(res.is_err(), "expected no message after unsubscribe");

  handle.abort();
}

#[tokio::test]
async fn test_heartbeat_registers_client_topics_for_delivery() {
  let self_addr = Address::from_str(&format!("127.0.0.1:{}", free_port())).unwrap();
  let server = Server::new(Vec::new(), self_addr.clone());
  let handle = tokio::spawn(async move { server.start().await });

  tokio::time::sleep(Duration::from_millis(150)).await;

  let (mut heartbeat_write, mut heartbeat_read) = connect_client(&self_addr).await.split();
  let (mut pub_write, _pub_read) = connect_client(&self_addr).await.split();

  heartbeat_write
    .send(tungstenite::Message::Binary(heartbeat_bytes(&["hb"])))
    .await
    .unwrap();
  tokio::time::sleep(Duration::from_millis(50)).await;

  pub_write
    .send(tungstenite::Message::Binary(publish_bytes("hb", b"pulse")))
    .await
    .unwrap();

  let publish = recv_publish(&mut heartbeat_read, Duration::from_secs(2))
    .await
    .expect("expected publish after heartbeat topic registration");
  assert_eq!(publish.topic, "hb");
  assert_eq!(publish.payload, b"pulse");

  handle.abort();
}

#[tokio::test]
async fn test_peer_forwarding_delivers_to_remote_subscribers() {
  // AI test
  let server1_addr = Address::from_str(&format!("127.0.0.1:{}", free_port())).unwrap();
  let server2_addr = Address::from_str(&format!("127.0.0.1:{}", free_port())).unwrap();

  let server1 = Server::new(vec![server2_addr.clone()], server1_addr.clone());
  let server2 = Server::new(vec![server1_addr.clone()], server2_addr.clone());

  let h1 = tokio::spawn(async move { server1.start().await });
  let h2 = tokio::spawn(async move { server2.start().await });

  tokio::time::sleep(Duration::from_millis(800)).await;

  let (mut sub_write, mut sub_read) = connect_client(&server2_addr).await.split();
  sub_write
    .send(tungstenite::Message::Binary(subscribe_bytes("shared")))
    .await
    .unwrap();

  tokio::time::sleep(HEARTBEAT_PROPAGATION_WAIT).await;

  let (mut pub_write, _pub_read) = connect_client(&server1_addr).await.split();
  pub_write
    .send(tungstenite::Message::Binary(publish_bytes(
      "shared",
      b"from-peer",
    )))
    .await
    .unwrap();

  let publish = recv_publish(&mut sub_read, Duration::from_secs(3))
    .await
    .expect("expected forwarded publish");
  assert_eq!(publish.topic, "shared");
  assert_eq!(publish.payload, b"from-peer");

  h1.abort();
  h2.abort();
}

#[tokio::test]
async fn test_peer_forwarding_between_two_servers_does_not_loop_or_duplicate() {
  // Repro for the "infinite redirect/bounce" bug: two servers that are peers,
  // both interested in the same topic, must not endlessly re-forward the same publish.
  let server1_addr = Address::from_str(&format!("127.0.0.1:{}", free_port())).unwrap();
  let server2_addr = Address::from_str(&format!("127.0.0.1:{}", free_port())).unwrap();

  let server1 = Server::new(vec![server2_addr.clone()], server1_addr.clone());
  let server2 = Server::new(vec![server1_addr.clone()], server2_addr.clone());

  let h1 = tokio::spawn(async move { server1.start().await });
  let h2 = tokio::spawn(async move { server2.start().await });

  tokio::time::sleep(Duration::from_millis(900)).await;

  // Subscribers on both servers so each advertises interest in the topic.
  let (mut sub1_w, mut sub1_r) = connect_client(&server1_addr).await.split();
  let (mut sub2_w, mut sub2_r) = connect_client(&server2_addr).await.split();

  sub1_w
    .send(tungstenite::Message::Binary(subscribe_bytes("loop")))
    .await
    .unwrap();
  sub2_w
    .send(tungstenite::Message::Binary(subscribe_bytes("loop")))
    .await
    .unwrap();

  // Let a heartbeat round propagate topic interest to peers.
  tokio::time::sleep(HEARTBEAT_PROPAGATION_WAIT).await;

  let (mut pub_w, _pub_r) = connect_client(&server1_addr).await.split();
  pub_w
    .send(tungstenite::Message::Binary(publish_bytes("loop", b"ping")))
    .await
    .unwrap();

  // Both local and remote should receive exactly one publish.
  let p1 = recv_publish(&mut sub1_r, Duration::from_secs(3))
    .await
    .expect("expected local publish delivery");
  let p2 = recv_publish(&mut sub2_r, Duration::from_secs(3))
    .await
    .expect("expected remote publish delivery");
  assert_eq!(p1.payload, b"ping");
  assert_eq!(p2.payload, b"ping");

  // Critically: no duplicates shortly after (would indicate a bounce loop).
  let dup_local = recv_publish(&mut sub1_r, Duration::from_millis(350)).await;
  let dup_remote = recv_publish(&mut sub2_r, Duration::from_millis(350)).await;
  assert!(
    dup_local.is_none(),
    "unexpected duplicate publish on origin server"
  );
  assert!(
    dup_remote.is_none(),
    "unexpected duplicate publish on remote server"
  );

  h1.abort();
  h2.abort();
}

#[tokio::test]
async fn test_reconnect_new_subscriber_receives_after_old_disconnect() {
  // AI test
  let self_addr = Address::from_str(&format!("127.0.0.1:{}", free_port())).unwrap();
  let server = Server::new(Vec::new(), self_addr.clone());
  let handle = tokio::spawn(async move { server.start().await });

  tokio::time::sleep(Duration::from_millis(150)).await;

  // First subscriber connects and subscribes, then disconnects.
  {
    let (mut w, _r) = connect_client(&self_addr).await.split();
    w.send(tungstenite::Message::Binary(subscribe_bytes("t3")))
      .await
      .unwrap();
    // drop connection
  }

  tokio::time::sleep(Duration::from_millis(50)).await;

  // New subscriber should still be able to subscribe and receive publishes.
  let (mut sub_write, mut sub_read) = connect_client(&self_addr).await.split();
  sub_write
    .send(tungstenite::Message::Binary(subscribe_bytes("t3")))
    .await
    .unwrap();

  tokio::time::sleep(Duration::from_millis(50)).await;

  let (mut pub_write, _pub_read) = connect_client(&self_addr).await.split();
  pub_write
    .send(tungstenite::Message::Binary(publish_bytes("t3", b"ok")))
    .await
    .unwrap();

  let publish = recv_publish(&mut sub_read, Duration::from_secs(2))
    .await
    .expect("expected publish");
  assert_eq!(publish.payload, b"ok");

  handle.abort();
}

#[tokio::test]
async fn test_two_subscribers_both_receive_local() {
  // AI test
  let self_addr = Address::from_str(&format!("127.0.0.1:{}", free_port())).unwrap();
  let server = Server::new(Vec::new(), self_addr.clone());
  let handle = tokio::spawn(async move { server.start().await });

  tokio::time::sleep(HEARTBEAT_PROPAGATION_WAIT).await;

  let (mut w1, mut r1) = connect_client(&self_addr).await.split();
  let (mut w2, mut r2) = connect_client(&self_addr).await.split();
  let (mut pub_w, _pub_r) = connect_client(&self_addr).await.split();

  w1.send(tungstenite::Message::Binary(subscribe_bytes("t").clone()))
    .await
    .unwrap();
  w2.send(tungstenite::Message::Binary(subscribe_bytes("t").clone()))
    .await
    .unwrap();
  tokio::time::sleep(Duration::from_millis(50)).await;

  pub_w
    .send(tungstenite::Message::Binary(publish_bytes("t", b"x")))
    .await
    .unwrap();

  let p1 = recv_publish(&mut r1, Duration::from_secs(2)).await;
  let p2 = recv_publish(&mut r2, Duration::from_secs(2)).await;
  assert!(p1.is_some() && p2.is_some());

  handle.abort();
}

#[tokio::test]
async fn test_topic_isolation_local_no_cross_delivery() {
  // AI test
  let self_addr = Address::from_str(&format!("127.0.0.1:{}", free_port())).unwrap();
  let server = Server::new(Vec::new(), self_addr.clone());
  let handle = tokio::spawn(async move { server.start().await });

  tokio::time::sleep(HEARTBEAT_PROPAGATION_WAIT).await;

  let (mut sub_w, mut sub_r) = connect_client(&self_addr).await.split();
  let (mut pub_w, _pub_r) = connect_client(&self_addr).await.split();

  sub_w
    .send(tungstenite::Message::Binary(subscribe_bytes("a")))
    .await
    .unwrap();
  tokio::time::sleep(Duration::from_millis(50)).await;

  pub_w
    .send(tungstenite::Message::Binary(publish_bytes("b", b"nope")))
    .await
    .unwrap();

  let res = tokio::time::timeout(Duration::from_millis(300), sub_r.next()).await;
  assert!(
    res.is_err(),
    "subscriber to topic a should not receive topic b"
  );

  handle.abort();
}

#[tokio::test]
async fn test_multi_topic_subscribe_receives_both() {
  // AI test
  let self_addr = Address::from_str(&format!("127.0.0.1:{}", free_port())).unwrap();
  let server = Server::new(Vec::new(), self_addr.clone());
  let handle = tokio::spawn(async move { server.start().await });

  tokio::time::sleep(Duration::from_millis(150)).await;

  let (mut sub_w, mut sub_r) = connect_client(&self_addr).await.split();
  let (mut pub_w, _pub_r) = connect_client(&self_addr).await.split();

  sub_w
    .send(tungstenite::Message::Binary(subscribe_bytes("a")))
    .await
    .unwrap();
  sub_w
    .send(tungstenite::Message::Binary(subscribe_bytes("b")))
    .await
    .unwrap();
  tokio::time::sleep(Duration::from_millis(50)).await;

  pub_w
    .send(tungstenite::Message::Binary(publish_bytes("a", b"1")))
    .await
    .unwrap();
  pub_w
    .send(tungstenite::Message::Binary(publish_bytes("b", b"2")))
    .await
    .unwrap();

  let p1 = recv_publish(&mut sub_r, Duration::from_secs(2))
    .await
    .expect("expected first publish");
  let p2 = recv_publish(&mut sub_r, Duration::from_secs(2))
    .await
    .expect("expected second publish");

  let mut topics = vec![p1.topic, p2.topic];
  topics.sort();
  assert_eq!(topics, vec!["a".to_string(), "b".to_string()]);

  handle.abort();
}

#[tokio::test]
async fn test_unsubscribe_one_topic_keeps_other() {
  // AI test
  let self_addr = Address::from_str(&format!("127.0.0.1:{}", free_port())).unwrap();
  let server = Server::new(Vec::new(), self_addr.clone());
  let handle = tokio::spawn(async move { server.start().await });

  tokio::time::sleep(Duration::from_millis(150)).await;

  let (mut sub_w, mut sub_r) = connect_client(&self_addr).await.split();
  let (mut pub_w, _pub_r) = connect_client(&self_addr).await.split();

  sub_w
    .send(tungstenite::Message::Binary(subscribe_bytes("a")))
    .await
    .unwrap();
  sub_w
    .send(tungstenite::Message::Binary(subscribe_bytes("b")))
    .await
    .unwrap();
  tokio::time::sleep(Duration::from_millis(50)).await;

  sub_w
    .send(tungstenite::Message::Binary(unsubscribe_bytes("a")))
    .await
    .unwrap();
  tokio::time::sleep(Duration::from_millis(50)).await;

  pub_w
    .send(tungstenite::Message::Binary(publish_bytes("a", b"no")))
    .await
    .unwrap();
  pub_w
    .send(tungstenite::Message::Binary(publish_bytes("b", b"yes")))
    .await
    .unwrap();

  // Should receive only topic b.
  let p = recv_publish(&mut sub_r, Duration::from_secs(2))
    .await
    .expect("expected publish for topic b");
  assert_eq!(p.topic, "b");
  assert_eq!(p.payload, b"yes");

  let res = tokio::time::timeout(Duration::from_millis(300), sub_r.next()).await;
  assert!(
    res.is_err(),
    "should not receive extra messages for unsubscribed topic"
  );

  handle.abort();
}

#[tokio::test]
async fn test_server_replies_pong_to_ping() {
  // AI test
  let self_addr = Address::from_str(&format!("127.0.0.1:{}", free_port())).unwrap();
  let server = Server::new(Vec::new(), self_addr.clone());
  let handle = tokio::spawn(async move { server.start().await });

  tokio::time::sleep(Duration::from_millis(150)).await;

  let (mut w, mut r) = connect_client(&self_addr).await.split();
  let payload: Bytes = Bytes::from_static(b"ping");
  w.send(tungstenite::Message::Ping(payload.clone()))
    .await
    .unwrap();

  let msg = tokio::time::timeout(Duration::from_secs(2), r.next())
    .await
    .unwrap()
    .unwrap()
    .unwrap();
  match msg {
    tungstenite::Message::Pong(p) => assert_eq!(p, payload),
    other => panic!("expected Pong, got: {:?}", other),
  }

  handle.abort();
}

#[tokio::test]
async fn test_server_survives_bad_client_message_and_accepts_new_clients() {
  // AI test
  let self_addr = Address::from_str(&format!("127.0.0.1:{}", free_port())).unwrap();
  let server = Server::new(Vec::new(), self_addr.clone());
  let handle = tokio::spawn(async move { server.start().await });

  tokio::time::sleep(Duration::from_millis(150)).await;

  // Bad client sends non-protobuf bytes (can crash that connection task).
  {
    let (mut w, _r) = connect_client(&self_addr).await.split();
    let _ = w
      .send(tungstenite::Message::Binary(Bytes::from_static(
        b"not-a-proto",
      )))
      .await;
  }

  // Server should still accept new clients and deliver messages.
  let (mut sub_w, mut sub_r) = connect_client(&self_addr).await.split();
  let (mut pub_w, _pub_r) = connect_client(&self_addr).await.split();

  sub_w
    .send(tungstenite::Message::Binary(subscribe_bytes("ok")))
    .await
    .unwrap();
  tokio::time::sleep(Duration::from_millis(50)).await;
  pub_w
    .send(tungstenite::Message::Binary(publish_bytes("ok", b"works")))
    .await
    .unwrap();

  let p = recv_publish(&mut sub_r, Duration::from_secs(2))
    .await
    .expect("expected publish after bad client");
  assert_eq!(p.payload, b"works");

  handle.abort();
}

#[tokio::test]
async fn test_peer_forwarding_multiple_subscribers_remote() {
  // AI test
  let server1_addr = Address::from_str(&format!("127.0.0.1:{}", free_port())).unwrap();
  let server2_addr = Address::from_str(&format!("127.0.0.1:{}", free_port())).unwrap();

  let server1 = Server::new(vec![server2_addr.clone()], server1_addr.clone());
  let server2 = Server::new(vec![server1_addr.clone()], server2_addr.clone());

  let h1 = tokio::spawn(async move { server1.start().await });
  let h2 = tokio::spawn(async move { server2.start().await });

  tokio::time::sleep(Duration::from_millis(800)).await;

  let (mut w1, mut r1) = connect_client(&server2_addr).await.split();
  let (mut w2, mut r2) = connect_client(&server2_addr).await.split();
  w1.send(tungstenite::Message::Binary(subscribe_bytes("shared2")))
    .await
    .unwrap();
  w2.send(tungstenite::Message::Binary(subscribe_bytes("shared2")))
    .await
    .unwrap();
  tokio::time::sleep(Duration::from_millis(150)).await;

  let (mut pub_w, _pub_r) = connect_client(&server1_addr).await.split();
  pub_w
    .send(tungstenite::Message::Binary(publish_bytes(
      "shared2", b"fanout",
    )))
    .await
    .unwrap();

  let p1 = recv_publish(&mut r1, Duration::from_secs(3)).await;
  let p2 = recv_publish(&mut r2, Duration::from_secs(3)).await;
  assert!(p1.is_some() && p2.is_some());

  h1.abort();
  h2.abort();
}

#[tokio::test]
async fn test_peer_forwarding_stops_after_remote_unsubscribe() {
  // AI test
  let server1_addr = Address::from_str(&format!("127.0.0.1:{}", free_port())).unwrap();
  let server2_addr = Address::from_str(&format!("127.0.0.1:{}", free_port())).unwrap();

  let server1 = Server::new(vec![server2_addr.clone()], server1_addr.clone());
  let server2 = Server::new(vec![server1_addr.clone()], server2_addr.clone());

  let h1 = tokio::spawn(async move { server1.start().await });
  let h2 = tokio::spawn(async move { server2.start().await });

  tokio::time::sleep(Duration::from_millis(800)).await;

  let (mut sub_w, mut sub_r) = connect_client(&server2_addr).await.split();
  sub_w
    .send(tungstenite::Message::Binary(subscribe_bytes("g")))
    .await
    .unwrap();
  tokio::time::sleep(Duration::from_millis(150)).await;

  let (mut pub_w, _pub_r) = connect_client(&server1_addr).await.split();
  pub_w
    .send(tungstenite::Message::Binary(publish_bytes("g", b"1")))
    .await
    .unwrap();
  let _ = recv_publish(&mut sub_r, Duration::from_secs(3))
    .await
    .expect("expected first forwarded publish");

  sub_w
    .send(tungstenite::Message::Binary(unsubscribe_bytes("g")))
    .await
    .unwrap();
  tokio::time::sleep(HEARTBEAT_PROPAGATION_WAIT).await;

  pub_w
    .send(tungstenite::Message::Binary(publish_bytes("g", b"2")))
    .await
    .unwrap();
  let res = recv_publish(&mut sub_r, Duration::from_millis(400)).await;
  assert!(
    res.is_none(),
    "expected no delivery after remote unsubscribe"
  );

  h1.abort();
  h2.abort();
}

#[tokio::test]
async fn test_publish_many_messages_delivered_count() {
  // AI test
  let self_addr = Address::from_str(&format!("127.0.0.1:{}", free_port())).unwrap();
  let server = Server::new(Vec::new(), self_addr.clone());
  let handle = tokio::spawn(async move { server.start().await });

  tokio::time::sleep(Duration::from_millis(150)).await;

  let (mut sub_w, mut sub_r) = connect_client(&self_addr).await.split();
  let (mut pub_w, _pub_r) = connect_client(&self_addr).await.split();

  sub_w
    .send(tungstenite::Message::Binary(subscribe_bytes("spam")))
    .await
    .unwrap();
  tokio::time::sleep(Duration::from_millis(50)).await;

  let n: usize = 25;
  for i in 0..n {
    pub_w
      .send(tungstenite::Message::Binary(publish_bytes(
        "spam",
        format!("m{}", i).as_bytes(),
      )))
      .await
      .unwrap();
  }

  let mut got = 0usize;
  let deadline = Duration::from_secs(5);
  let start = tokio::time::Instant::now();
  while got < n && start.elapsed() < deadline {
    if recv_publish(&mut sub_r, Duration::from_millis(500))
      .await
      .is_some()
    {
      got += 1;
    }
  }
  assert_eq!(got, n, "expected all publishes to be delivered");

  handle.abort();
}

#[tokio::test]
async fn test_same_payload_published_twice_delivers_twice() {
  // Ensure we do NOT dedupe by (topic,payload); only by msg_id.
  let self_addr = Address::from_str(&format!("127.0.0.1:{}", free_port())).unwrap();
  let server = Server::new(Vec::new(), self_addr.clone());
  let handle = tokio::spawn(async move { server.start().await });

  tokio::time::sleep(Duration::from_millis(150)).await;

  let (mut sub_w, mut sub_r) = connect_client(&self_addr).await.split();
  let (mut pub_w, _pub_r) = connect_client(&self_addr).await.split();

  sub_w
    .send(tungstenite::Message::Binary(subscribe_bytes("dup")))
    .await
    .unwrap();
  tokio::time::sleep(Duration::from_millis(50)).await;

  pub_w
    .send(tungstenite::Message::Binary(publish_bytes("dup", b"same")))
    .await
    .unwrap();
  pub_w
    .send(tungstenite::Message::Binary(publish_bytes("dup", b"same")))
    .await
    .unwrap();

  let p1 = recv_publish(&mut sub_r, Duration::from_secs(2))
    .await
    .expect("expected first publish");
  let p2 = recv_publish(&mut sub_r, Duration::from_secs(2))
    .await
    .expect("expected second publish");

  assert_eq!(p1.topic, "dup");
  assert_eq!(p2.topic, "dup");
  assert_eq!(p1.payload, b"same");
  assert_eq!(p2.payload, b"same");

  handle.abort();
}

#[tokio::test]
async fn test_forwarding_is_transitive_across_peers() {
  // AI test
  let server1_addr = Address::from_str(&format!("127.0.0.1:{}", free_port())).unwrap();
  let server2_addr = Address::from_str(&format!("127.0.0.1:{}", free_port())).unwrap();
  let server3_addr = Address::from_str(&format!("127.0.0.1:{}", free_port())).unwrap();

  // Topology: server1 <-> server2 <-> server3 (server1 does not know server3).
  let server1 = Server::new(vec![server2_addr.clone()], server1_addr.clone());
  let server2 = Server::new(
    vec![server1_addr.clone(), server3_addr.clone()],
    server2_addr.clone(),
  );
  let server3 = Server::new(vec![server2_addr.clone()], server3_addr.clone());

  let h1 = tokio::spawn(async move { server1.start().await });
  let h2 = tokio::spawn(async move { server2.start().await });
  let h3 = tokio::spawn(async move { server3.start().await });

  tokio::time::sleep(Duration::from_millis(900)).await;

  // Make server2 "interested" in mh so server1 forwards to it.
  let (mut dummy_w, mut dummy_r) = connect_client(&server2_addr).await.split();
  dummy_w
    .send(tungstenite::Message::Binary(subscribe_bytes("mh")))
    .await
    .unwrap();

  // Real subscriber on server3 to mh.
  let (mut sub3_w, mut sub3_r) = connect_client(&server3_addr).await.split();
  sub3_w
    .send(tungstenite::Message::Binary(subscribe_bytes("mh")))
    .await
    .unwrap();

  tokio::time::sleep(HEARTBEAT_PROPAGATION_WAIT).await;

  let (mut pub_w, _pub_r) = connect_client(&server1_addr).await.split();
  pub_w
    .send(tungstenite::Message::Binary(publish_bytes(
      "mh", b"one-hop",
    )))
    .await
    .unwrap();

  // Dummy on server2 should receive (server1 -> server2 forwarding works).
  let got_dummy = recv_publish(&mut dummy_r, Duration::from_secs(3)).await;
  assert!(
    got_dummy.is_some(),
    "expected server2 local delivery via forwarding"
  );

  // Server3 should NOT receive, because ServerForward messages are delivered locally only
  // and are not forwarded onward.
  let got_server3 = recv_publish(&mut sub3_r, Duration::from_millis(400)).await;
  assert!(
    got_server3.is_none(),
    "unexpected transitive forwarding to server3; ServerForward should not be re-forwarded"
  );

  h1.abort();
  h2.abort();
  h3.abort();
}
