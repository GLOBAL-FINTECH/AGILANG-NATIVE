//! Native realtime transports for AGILANG applications.
use agilang_runtime_core::{AgilangError, ErrorCode, RuntimeResult};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{accept_async, connect_async, tungstenite::Message, WebSocketStream};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealtimeMessage {
    pub topic: String,
    pub payload: Vec<u8>,
}

pub struct WebSocketClient {
    stream: WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>,
}
impl WebSocketClient {
    pub async fn connect(url: &str) -> RuntimeResult<Self> {
        let (stream, _) = connect_async(url).await.map_err(network)?;
        Ok(Self { stream })
    }
    pub async fn send_binary(&mut self, payload: Vec<u8>) -> RuntimeResult<()> {
        self.stream
            .send(Message::Binary(payload.into()))
            .await
            .map_err(network)
    }
    pub async fn send_text(&mut self, payload: String) -> RuntimeResult<()> {
        self.stream
            .send(Message::Text(payload.into()))
            .await
            .map_err(network)
    }
    pub async fn receive(&mut self) -> RuntimeResult<Option<Vec<u8>>> {
        match self.stream.next().await {
            Some(Ok(Message::Binary(v))) => Ok(Some(v.to_vec())),
            Some(Ok(Message::Text(v))) => Ok(Some(v.as_bytes().to_vec())),
            Some(Ok(Message::Close(_))) | None => Ok(None),
            Some(Ok(_)) => Ok(Some(Vec::new())),
            Some(Err(e)) => Err(network(e)),
        }
    }
}

pub async fn serve_websocket<F, Fut>(address: &str, handler: F) -> RuntimeResult<()>
where
    F: Fn(WebSocketStream<TcpStream>) -> Fut + Clone + Send + Sync + 'static,
    Fut: std::future::Future<Output = RuntimeResult<()>> + Send + 'static,
{
    let listener = TcpListener::bind(address).await.map_err(network)?;
    loop {
        let (stream, _) = listener.accept().await.map_err(network)?;
        let handler = handler.clone();
        tokio::spawn(async move {
            if let Ok(ws) = accept_async(stream).await {
                let _ = handler(ws).await;
            }
        });
    }
}
fn network(e: impl std::fmt::Display) -> AgilangError {
    AgilangError::new(ErrorCode::Io, e.to_string())
}
