use bytes::Bytes;
use futures_util::SinkExt;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use crate::util::Address;
pub mod sendable_readable;

pub enum WebsockType {
    Client,
    Server(Option<String>),
}

pub struct OpenedWebsocket {
    ws: Option<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    ws_type: WebsockType,
}

impl OpenedWebsocket {
    pub fn new(ws: WebSocketStream<TcpStream>, ws_type: WebsockType) -> Self {
        Self {
            ws: Some(ws),
            ws_type,
        }
    }

    pub async fn new_client(ws: WebSocketStream<TcpStream>) -> Self {
        Self {
            ws: Some(ws),
            ws_type: WebsockType::Client,
        }
    }

    pub async fn new_server(self, addr: Address) -> Self {
        if let Ok((ws_stream, _)) = connect_async(addr.build_ws_url()).await {
            self.ws = Some(ws_stream);
        }

        let (_, read) = self.websocket.as_mut().unwrap();

        let server_state = timeout(Duration::from_secs(1), read.next())
            .await
            .unwrap_or_else(|_| {
                error!("Error with server_state where timeout was reached");
                return None;
            })
            .and_then(|msg| {
                debug!("Received server state message");
                msg.ok()
            })
            .and_then(|msg| {
                let data = msg.into_data();
                debug!("Decoding server state message of {} bytes", data.len());
                ServerInitMessage::decode(Bytes::from(data)).ok()
            })
            .expect("Error with server_state");

        if server_state.message_type == ServerMessageType::Init as i32 {
            self.server_id = Some(server_state.uuid);
        } else {
            return false;
        }

        Self {
            ws: ws_stream.split(),
            ws_type: WebsockType::Server(Some(server_state.uuid)),
        }
    }

    pub async fn send(&mut self, msg: Bytes) -> Result<(), Box<dyn std::error::Error>> {
        self.ws.send(Message::Binary(msg)).await?;
        Ok(())
    }

    pub async fn close(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.ws.close(None).await?;
        Ok(())
    }
}
