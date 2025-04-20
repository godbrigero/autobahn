use std::fmt::Display;

#[derive(serde::Deserialize, Clone, Hash, PartialEq, Eq, Debug)]
pub struct Address {
  pub host: String,
  pub port: i32,
}

impl Display for Address {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}:{}", self.host, self.port)
  }
}

impl Address {
  pub fn build_ws_url(&self) -> String {
    return format!("ws://{}:{}", self.host, self.port);
  }

  pub fn from_str(s: &str) -> Result<Address, String> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
      return Err("Invalid address format".to_string());
    }
    Ok(Address {
      host: parts[0].to_string(),
      port: parts[1].parse().map_err(|_| "Invalid port number")?,
    })
  }

  pub fn to_string(&self) -> String {
    return format!("{}:{}", self.host, self.port);
  }
}

pub mod proto {

  use bytes::{Bytes, BytesMut};
  use prost::Message;

  use crate::message::{MessageType, ServerStateMessage};

  pub fn build_server_state_message(topics: Vec<String>, uuid: String) -> Bytes {
    return build_proto_message(&ServerStateMessage {
      message_type: MessageType::ServerState as i32,
      topics,
      uuid,
    });
  }

  pub fn build_proto_message<T: Message>(message: &T) -> Bytes {
    return {
      let mut buf = BytesMut::new();
      let _ = message.encode(&mut buf);
      buf.freeze()
    };
  }

  pub fn deserialize_partial<T: Default + Message>(buf: &[u8]) -> Result<T, prost::DecodeError> {
    let mut instance = T::default();
    let mut cursor = buf;
    instance.merge(&mut cursor)?;
    Ok(instance)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::str::FromStr;

  #[test]
  fn test_address_from_str() {
    let addr = Address::from_str("127.0.0.1:8080").unwrap();
    assert_eq!(addr.host, "127.0.0.1");
    assert_eq!(addr.port, 8080);
  }

  #[test]
  fn test_address_to_string() {
    let addr = Address {
      host: "127.0.0.1".to_string(),
      port: 8080,
    };
    assert_eq!(addr.to_string(), "127.0.0.1:8080");
  }

  // Add more test functions for other utility functions
}
