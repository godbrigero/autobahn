use bytes::{Bytes, BytesMut};
use log::error;
use prost::Message;

use crate::message::{AbstractMessage, MessageType};

pub fn build_proto_message<T: Message>(message: &T) -> Bytes {
  return {
    let mut buf = BytesMut::new();
    let _ = message.encode(&mut buf);
    buf.freeze()
  };
}

pub fn get_message_type(message: &Bytes) -> MessageType {
  let abstract_message = AbstractMessage::decode(message.clone()).unwrap();
  return MessageType::try_from(abstract_message.message_type).unwrap();
}

pub fn get_message<T: Message + Default>(message: &Bytes) -> T {
  let output = T::decode(message.clone());
  if output.is_err() {
    error!("Failed to decode message: {:?}", output.err());
    return T::default();
  }

  return output.unwrap();
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::message::{MessageType, PublishMessage};

  #[test]
  fn test_get_message_type_publish() {
    let msg = PublishMessage {
      message_type: MessageType::Publish as i32,
      topic: "t".to_string(),
      payload: b"p".to_vec(),
      ..Default::default()
    };
    let bytes = build_proto_message(&msg);
    assert_eq!(get_message_type(&bytes), MessageType::Publish);
  }

  #[test]
  fn test_get_message_success() {
    let msg = PublishMessage {
      message_type: MessageType::Publish as i32,
      topic: "t".to_string(),
      payload: b"p".to_vec(),
      ..Default::default()
    };
    let bytes = build_proto_message(&msg);
    let decoded: PublishMessage = get_message(&bytes);
    assert_eq!(decoded.topic, "t");
    assert_eq!(decoded.payload, b"p");
  }

  #[test]
  fn test_get_message_failure_returns_default() {
    let bad = Bytes::from_static(b"not-a-proto");
    let decoded: PublishMessage = get_message(&bad);
    assert_eq!(decoded, PublishMessage::default());
  }
}
