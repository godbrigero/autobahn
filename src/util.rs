#[derive(serde::Deserialize, Clone, Hash, PartialEq, Eq, Debug)]
pub struct Address {
  pub host: String,
  pub port: i32,
}

impl Address {
  pub fn build_ws_url(&self) -> String {
    return format!("ws://{}:{}", self.host, self.port);
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
