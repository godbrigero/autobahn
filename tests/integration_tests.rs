use autobahn::server::Server;
use autobahn::util::Address;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::connect_async;

#[tokio::test]
async fn test_server_startup() {
  let self_addr = Address::from_str("127.0.0.1:8080").unwrap();
  let mut server = Arc::new(Server::new(Vec::new(), self_addr.clone()));

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

  let mut server1 = Arc::new(Server::new(
    vec![server2_addr.clone()],
    server1_addr.clone(),
  ));
  let mut server2 = Arc::new(Server::new(
    vec![server1_addr.clone()],
    server2_addr.clone(),
  ));

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
