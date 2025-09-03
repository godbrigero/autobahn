pub mod router;
pub mod server;
pub mod util;
pub mod websocket;

pub mod message {
    include!(concat!(env!("OUT_DIR"), "/message.rs"));
}
