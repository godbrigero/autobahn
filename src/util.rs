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

  ///
  /// This function is used to get the message type from a buffer.
  /// It is about 2-3x faster than the deserialize_partial function.
  /// I implemented this before realizing that deserialization only takes around
  /// 600um.... for deserialize_partial function. This one takes around 300um.
  /// However, there are spikes so, sometimes, deserialize_partial can take like 0um
  /// Idk what it is dependant on.
  ///
  /// ### Proof:
  /// ```bash
  /// Time taken proto::deserialize_partial: 209ns
  /// Time taken proto::fast_get_message_type: 41ns
  /// ```
  ///
  /// # Purpose
  ///
  /// The fastest way to get the message type from a buffer.
  ///
  /// # Returns
  ///
  /// Returns the message type if it is a valid message type.
  ///
  /// # Examples
  ///
  /// ```
  /// use autobahn::message::MessageType;
  /// use autobahn::util::proto::fast_get_message_type;
  ///
  /// // Example 1: Field not present (defaults to ServerState)
  /// let buf = [0x12, 0x00];  // Some other field
  /// let message_type = fast_get_message_type(&buf);
  /// assert_eq!(message_type, Some(MessageType::ServerState));
  ///
  /// // Example 2: Explicit message type
  /// let buf = [0x08, 0x02];  // Field 1 with value 2 (Publish)
  /// let message_type = fast_get_message_type(&buf);
  /// assert_eq!(message_type, Some(MessageType::Publish));
  /// ```
  ///
  pub fn fast_get_message_type(buf: &[u8]) -> Option<MessageType> {
    if buf.is_empty() {
      return None;
    }

    if buf.len() < 2 {
      return None;
    }

    let field_number = buf[0] >> 3;

    // If field 1 isn't present, it means message_type is the default value (0 = ServerState)
    if field_number != 1 {
      return Some(MessageType::ServerState);
    }

    let wire_type = buf[0] & 0x07;
    if wire_type != 0 {
      return None;
    }

    let mut value = 0i32;
    let mut shift = 0;
    let mut idx = 1;

    while idx < buf.len() {
      let byte = buf[idx];
      value |= ((byte & 0x7F) as i32) << shift;

      if (byte & 0x80) == 0 {
        break;
      }

      shift += 7;
      idx += 1;

      if shift >= 32 {
        return None;
      }
    }

    let result = MessageType::try_from(value).ok();
    result
  }
}

#[cfg(test)]
mod tests {
  use bytes::Bytes;
  use tokio::time::Instant;

  use crate::{
    message::{AbstractMessage, MessageType, PublishMessage, ServerStateMessage},
    util::{proto, Address},
  };

  fn generate_random_bytes_number(num: usize) -> Vec<u8> {
    (0..num).map(|_| rand::random::<u8>()).collect()
  }

  #[test]
  fn test_address_from_str() {
    let addr = Address::from_str("127.0.0.1:8080").unwrap();
    assert_eq!(addr.host, "127.0.0.1");
    assert_eq!(addr.port, 8080);
  }

  #[test]
  fn test_time_deserialize() {
    let message = PublishMessage {
      message_type: MessageType::Publish as i32,
      topic: "test".to_string(),
      payload: generate_random_bytes_number(480000),
    };

    let bytes = proto::build_proto_message(&message);

    let start = Instant::now();
    let message = proto::deserialize_partial::<AbstractMessage>(&bytes);
    let duration_proto = start.elapsed();
    println!(
      "Time taken proto::deserialize_partial: {:?}",
      duration_proto
    );

    let start = Instant::now();
    let message = proto::fast_get_message_type(&bytes);
    let duration_fast_get_message_type = start.elapsed();
    println!(
      "Time taken proto::fast_get_message_type: {:?}",
      duration_fast_get_message_type
    );

    assert!(duration_fast_get_message_type <= duration_proto);
  }

  #[test]
  fn test_address_to_string() {
    let addr = Address {
      host: "127.0.0.1".to_string(),
      port: 8080,
    };
    assert_eq!(addr.to_string(), "127.0.0.1:8080");
  }

  fn get_publish_message(msg_type: MessageType) -> PublishMessage {
    return PublishMessage {
      message_type: msg_type as i32,
      topic: "test".to_string(),
      payload: vec![],
    };
  }

  fn fast_deserialize(msg_type: MessageType) -> Option<MessageType> {
    let proto_message = get_publish_message(msg_type);
    let bytes = proto::build_proto_message(&proto_message);
    let message_type = proto::fast_get_message_type(&bytes);
    return message_type;
  }

  #[test]
  fn test_fast_get_message_type() {
    assert_eq!(
      fast_deserialize(MessageType::ServerState),
      Some(MessageType::ServerState)
    );
    assert_eq!(
      fast_deserialize(MessageType::ServerForward),
      Some(MessageType::ServerForward)
    );
    assert_eq!(
      fast_deserialize(MessageType::Unsubscribe),
      Some(MessageType::Unsubscribe)
    );
    assert_eq!(
      fast_deserialize(MessageType::Subscribe),
      Some(MessageType::Subscribe)
    );
    assert_eq!(
      fast_deserialize(MessageType::ServerStats),
      Some(MessageType::ServerStats)
    );
    assert_eq!(
      fast_deserialize(MessageType::Publish),
      Some(MessageType::Publish)
    );
  }

  // Add more test functions for other utility functions
}
