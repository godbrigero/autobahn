pub mod config;
pub mod server;
pub mod util;

mod message {
  include!(concat!(env!("OUT_DIR"), "/message.rs"));
}
