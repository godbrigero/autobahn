pub mod config;
pub mod discovery;
pub mod server;
pub mod util;

pub mod message {
  include!(concat!(env!("OUT_DIR"), "/message.rs"));
}
