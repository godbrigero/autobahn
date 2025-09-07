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

  let server = Server::new(config.others.unwrap_or(vec![]), config.self_addr);

  let result = tokio::select! {
    _ = server.start() => "Server exited",
    _ = ctrl_c() => "Received Ctrl+C"
  };
  println!("Exiting: {}", result);

  println!("Shutting down services...");
}
