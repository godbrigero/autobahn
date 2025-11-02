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

  let server = Server::new(config.others.unwrap_or(vec![]), config.self_addr.clone());
  let discovery = Discovery::new(config.self_addr.port as u16);
  let server_clone = server.clone();

  let discovery_future = discovery.clone().start_discovery_loop(move |addr, port| {
    let server_clone = server_clone.clone();
    async move {
      let new_peer = Address::new(addr, port as i32);
      server_clone.add_peer(new_peer).await;
    }
  });

  let result = tokio::select! {
    _ = server.start() => "Server exited",
    _ = discovery.run_discovery_server_continuous() => "Discovery server stopped",
    _ = discovery_future => "Discovery client stopped",
    _ = ctrl_c() => "Received Ctrl+C"
  };
  println!("Exiting: {}", result);

  println!("Shutting down services...");
}
