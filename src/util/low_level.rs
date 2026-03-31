use std::ops::Deref;

use bytes::{Buf, Bytes};
use no_incode_comments::external_doc;
use prost::Message;

#[derive(Clone)]
#[external_doc(path = "docs/util/low_level.md", key = "ExpectedTypedBytes")]
pub struct ExpectedTypedBytes<T: Message + Default> {
  bytes: Bytes,
  _phantom: std::marker::PhantomData<T>,
}

impl<T: Message + Default> Deref for ExpectedTypedBytes<T> {
  type Target = Bytes;

  #[external_doc(path = "docs/util/low_level.md", key = "ExpectedTypedBytes::deref")]
  fn deref(&self) -> &Self::Target {
    &self.bytes
  }
}

impl<T: Message + Default> Buf for ExpectedTypedBytes<T> {
  #[external_doc(path = "docs/util/low_level.md", key = "ExpectedTypedBytes::remaining")]
  fn remaining(&self) -> usize {
    self.bytes.remaining()
  }

  #[external_doc(path = "docs/util/low_level.md", key = "ExpectedTypedBytes::chunk")]
  fn chunk(&self) -> &[u8] {
    self.bytes.chunk()
  }

  #[external_doc(path = "docs/util/low_level.md", key = "ExpectedTypedBytes::advance")]
  fn advance(&mut self, cnt: usize) {
    self.bytes.advance(cnt)
  }
}

impl<T: Message + Default> AsRef<[u8]> for ExpectedTypedBytes<T> {
  #[external_doc(path = "docs/util/low_level.md", key = "ExpectedTypedBytes::as_ref")]
  fn as_ref(&self) -> &[u8] {
    self.bytes.as_ref()
  }
}

impl<T: Message + Default> From<ExpectedTypedBytes<T>> for Bytes {
  #[external_doc(
    path = "docs/util/low_level.md",
    key = "ExpectedTypedBytes::into_bytes"
  )]
  fn from(typed_bytes: ExpectedTypedBytes<T>) -> Self {
    typed_bytes.bytes
  }
}

impl<T: Message + Default> From<Bytes> for ExpectedTypedBytes<T> {
  #[external_doc(
    path = "docs/util/low_level.md",
    key = "ExpectedTypedBytes::from_bytes"
  )]
  fn from(bytes: Bytes) -> Self {
    ExpectedTypedBytes {
      bytes,
      _phantom: std::marker::PhantomData,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::message::{MessageType, PublishMessage};

  fn sample_publish() -> PublishMessage {
    PublishMessage {
      message_type: MessageType::Publish as i32,
      topic: "topic".to_string(),
      payload: b"payload".to_vec(),
    }
  }

  #[test]
  fn test_from_bytes_and_deref_expose_original_bytes() {
    let mut raw = bytes::BytesMut::new();
    sample_publish().encode(&mut raw).unwrap();
    let raw = raw.freeze();

    let typed = ExpectedTypedBytes::<PublishMessage>::from(raw.clone());

    assert_eq!(typed.as_ref(), raw.as_ref());
    assert_eq!(&*typed, &raw);
  }

  #[test]
  fn test_into_bytes_returns_original_bytes() {
    let mut raw = bytes::BytesMut::new();
    sample_publish().encode(&mut raw).unwrap();
    let raw = raw.freeze();

    let typed = ExpectedTypedBytes::<PublishMessage>::from(raw.clone());
    let converted: Bytes = typed.into();

    assert_eq!(converted, raw);
  }

  #[test]
  fn test_buf_impl_reads_and_advances_underlying_bytes() {
    let raw = Bytes::from_static(b"abc");
    let mut typed = ExpectedTypedBytes::<PublishMessage>::from(raw);

    assert_eq!(typed.remaining(), 3);
    assert_eq!(typed.chunk(), b"abc");

    typed.advance(1);

    assert_eq!(typed.remaining(), 2);
    assert_eq!(typed.chunk(), b"bc");
    assert_eq!(typed.as_ref(), b"bc");
  }
}
