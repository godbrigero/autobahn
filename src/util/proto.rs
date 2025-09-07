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
