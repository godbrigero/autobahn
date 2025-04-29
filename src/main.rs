use no_incode_comments::external_doc;

use std::{env, sync::Arc};

use config::Config;
use server::Server;

mod config;
mod global_connection_server;
mod server;
mod util;

mod message {
  include!(concat!(env!("OUT_DIR"), "/message.rs"));
}

mod message_v2 {
  include!(concat!(env!("OUT_DIR"), "/server_message.rs"));
}

#[tokio::main]
async fn main() {
  let config = Config::from_args(env::args().collect());
  if config.debug {
    println!("Debug mode enabled");
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
  }

  let server = Arc::new(Server::new(config.others, config.self_addr));
  server.start().await;
}
