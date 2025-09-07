use autobahn::discovery::Discovery;
use autobahn::server::Server;
use autobahn::util::Address;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::sleep;
use tokio_tungstenite::connect_async;

#[tokio::test]
async fn test_server_startup() {
  let self_addr = Address::from_str("127.0.0.1:8080").unwrap();
  let mut server = Server::new(Vec::new(), self_addr.clone());

  // Start server in a separate task
  let server_handle = tokio::spawn(async move {
    server.start().await;
  });

  // Give server time to start
  tokio::time::sleep(Duration::from_millis(100)).await;

  // Try to connect to the server
  let url = format!("ws://{}", self_addr);
  let result = connect_async(url).await;
  assert!(result.is_ok());

  server_handle.abort();
}

#[tokio::test]
async fn test_peer_communication() {
  let server1_addr = Address::from_str("127.0.0.1:8081").unwrap();
  let server2_addr = Address::from_str("127.0.0.1:8082").unwrap();

  let mut server1 = Server::new(vec![server2_addr.clone()], server1_addr.clone());
  let mut server2 = Server::new(vec![server1_addr.clone()], server2_addr.clone());

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
  let found_ports: Arc<Mutex<Vec<u16>>> = Arc::new(Mutex::new(Vec::new()));
  let discovery_1 = Arc::new(Discovery::new_with_ip("127.0.0.1".to_string(), 8080));
  let discovery_2 = Arc::new(Discovery::new(8081));

  let found_ports_clone = found_ports.clone();
  let discovery_1_handle = tokio::spawn(discovery_1.start_discovery_loop(move |ip, port| {
    let found_ports = found_ports_clone.clone();
    async move {
      let mut found_ports = found_ports.lock().await;
      found_ports.push(port);
    }
  }));

  discovery_2.run_discovery_server();

  sleep(Duration::from_millis(1000)).await;

  discovery_1_handle.abort();

  let found_ports = found_ports.lock().await;
  assert_eq!(found_ports.len(), 1);
  assert_eq!(found_ports[0], 8081);
}
