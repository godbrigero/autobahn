# Peer

## Description

The `Peer` struct represents a connection to a remote server. It handles the establishment of a WebSocket connection, message sending, and receiving to a particular server that is identified by a UUID. The peer is responsible for sending and receiving messages to the server as well as identifying the correct UUID for the peer.

Please note that the peer is NOT responsible for checking if the server is actually the correct server. This means that an external process must periodically call the `ensure_connection` method to ensure that the peer is still connected to the server.

## Fields

- `websocket`: A `WebSocket` struct that handles the WebSocket connection.
- `server_id`: A `String` that represents the UUID of the server note that initially this is `None` and will be set when the peer receives the `ServerInitMessage` from the server.
- `address`: A `Address` struct that represents the address of the server.





# ensure_connection

## Description

This method ensures that the peer is connected to the server. If the peer is not connected, it will attempt to connect to the server. If the peer is already connected, it will do nothing.

## Returns

- `true` if the peer is connected to the server.
- `false` if the peer is not connected to the server.

## Example

```rust
let mut peer = Peer::new(address);
peer.ensure_connection().await;
```

## Throws 

- `Error` if the peer is not connected to the server. Or if something went wrong with the server initialize message.



# send

## Description

This method sends a message to the server. Please note that this method will return `false` if the peer is not connected to the server WHICH IS CHECKED BY THE `is_connected` METHOD.

## Returns

- `true` if the message is sent to the server.
- `false` if the message is not sent to the server. This will happen if the peer is not connected to the server or if the message is not sent correctly (connection error).

# is_connected

## Description

This method checks if the peer is connected to the server. Note that this method works by checking if the `websocket` field is `Some` meaning that it does not automatically try to reconnect to the server if the connection is lost.

## Returns

- `true` if the peer is connected to the server.
- `false` if the peer is not connected to the server.
