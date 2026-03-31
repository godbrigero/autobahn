# ExpectedTypedBytes

**Source:** [`low_level.rs`](low_level.rs) (symlink to [`src/util/low_level.rs`](../../src/util/low_level.rs)).

A zero-cost wrapper around [`bytes::Bytes`] that carries a compile-time type parameter `T: prost::Message + Default` without storing a `T` value. The inner buffer is plain bytes; [`PhantomData`](std::marker::PhantomData) only associates those bytes with the protobuf message type handlers expect for a given code path.

## Role in the server

[`ServerMessageHandler`](crate::server::ServerMessageHandler) is implemented once per concrete message type. Its `handle` method takes `ExpectedTypedBytes<T>` so each implementation is tied to one prost type (for example [`PublishMessage`](crate::message::PublishMessage), [`TopicMessage`](crate::message::TopicMessage), [`HeartbeatMessage`](crate::message::HeartbeatMessage)).

Incoming WebSocket frames are untyped [`Bytes`](bytes::Bytes). The server decodes an envelope to obtain a [`MessageType`](crate::message::MessageType), then dispatches by wrapping the same buffer in `ExpectedTypedBytes::<ThatMessage>::from(bytes)` and calling [`ServerMessageHandler::handle`](crate::server::ServerMessageHandler::handle) on [`Server`](crate::server::Server). That selects the right handler at compile time and documents which protobuf shape each path uses.

## Semantics

This type does **not** parse or validate the buffer when constructed. Correct pairing of `MessageType` and `T` is enforced by the dispatch `match` that chooses which `ExpectedTypedBytes::<T>` to build. Handlers decode with `T::decode` or pass the bytes through (for example forwarding) as appropriate.

Per-method details are under the keys below (for example `# ExpectedTypedBytes::deref`).

# ExpectedTypedBytes::deref

[`Deref`](std::ops::Deref) implementation so `ExpectedTypedBytes<T>` dereferences to the inner [`Bytes`](bytes::Bytes).

Use this to call [`Bytes`](bytes::Bytes) methods or pass `&Bytes` where needed. Coercion also applies `&ExpectedTypedBytes<T>` → `&Bytes` for method resolution on the shared slice API.

# ExpectedTypedBytes::remaining

[`Buf::remaining`](bytes::Buf::remaining) for the inner buffer: number of bytes not yet consumed.

Delegates to [`Bytes::remaining`](bytes::Bytes). Together with `chunk` and `advance`, this lets you treat the wrapper as a [`Buf`](bytes::Buf) without unwrapping.

# ExpectedTypedBytes::chunk

[`Buf::chunk`](bytes::Buf::chunk): returns a slice view of the next contiguous chunk of unread bytes.

Delegates to the inner [`Bytes`](bytes::Bytes). Useful for zero-copy inspection before decoding or forwarding.

# ExpectedTypedBytes::advance

[`Buf::advance`](bytes::Buf::advance): drops the first `cnt` bytes from the logical read cursor on this buffer.

Delegates to [`Bytes::advance`](bytes::Bytes::advance). Mutates the wrapper in place; does not reallocate.

# ExpectedTypedBytes::as_ref

[`AsRef<[u8]>`](std::convert::AsRef) implementation: borrows the full buffer as a byte slice.

Delegates to [`Bytes::as_ref`](bytes::Bytes::as_ref). Handy for [`prost::Message::decode`](prost::Message::decode), comparisons, or copying without going through `deref`.

# ExpectedTypedBytes::from_bytes

[`From<Bytes>`](std::convert::From) for `ExpectedTypedBytes<T>`: wraps an existing [`Bytes`](bytes::Bytes) with the phantom type `T`.

This is what the server uses after routing by [`MessageType`](crate::message::MessageType), for example `ExpectedTypedBytes::<PublishMessage>::from(bytes)`. No decoding runs here; `T` is only a compile-time label.

# ExpectedTypedBytes::into_bytes

[`From<ExpectedTypedBytes<T>>`](std::convert::From) for [`Bytes`](bytes::Bytes): extracts the inner buffer and drops the phantom type.

Consumes the wrapper and returns the owned [`Bytes`](bytes::Bytes) with no extra allocation. Use when an API needs untyped bytes again.
