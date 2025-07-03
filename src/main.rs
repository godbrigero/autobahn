use tokio::join;
use tokio::signal::ctrl_c;

use std::{env, sync::Arc};

use autobahn::config::Config;
use autobahn::discovery::Discovery;
use autobahn::server::Server;
use autobahn::util::Address;

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
  let config = Config::from_args(env::args().collect());
  if config.debug {
    println!("Debug mode enabled");
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
  }

  let discovery = Arc::new(Discovery::new(config.self_addr.port as u16));
  let server = Arc::new(Server::new(
    config.others.unwrap_or(vec![]),
    config.self_addr,
  ));
  let server_clone = server.clone();
  discovery.clone().run_discovery_server();
  let discovery_loop_handle = discovery.clone().start_discovery_loop(move |ip, port| {
    let server_clone = server_clone.clone();
    async move {
      server_clone
        .add_peer(Address {
          host: ip,
          port: port as i32,
        })
        .await;
    }
  });
  let server_handle = server.start();

  let result = tokio::select! {
    _ = discovery_loop_handle => "Discovery loop exited",
    _ = server_handle => "Server exited",
    _ = ctrl_c() => "Received Ctrl+C"
  };
  println!("Exiting: {}", result);

  println!("Shutting down services...");
}
