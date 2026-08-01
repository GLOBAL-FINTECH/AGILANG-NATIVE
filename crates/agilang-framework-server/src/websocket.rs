use base64::{engine::general_purpose::STANDARD, Engine as _};
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::io::{self, Read, Write};

const WEBSOCKET_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
pub const DEFAULT_MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebSocketOpcode {
    Continuation,
    Text,
    Binary,
    Close,
    Ping,
    Pong,
}

impl WebSocketOpcode {
    fn from_wire(value: u8) -> Result<Self, WebSocketError> {
        match value {
            0x0 => Ok(Self::Continuation),
            0x1 => Ok(Self::Text),
            0x2 => Ok(Self::Binary),
            0x8 => Ok(Self::Close),
            0x9 => Ok(Self::Ping),
            0xA => Ok(Self::Pong),
            opcode => Err(WebSocketError::Protocol(format!(
                "unsupported WebSocket opcode 0x{opcode:02x}"
            ))),
        }
    }

    fn to_wire(self) -> u8 {
        match self {
            Self::Continuation => 0x0,
            Self::Text => 0x1,
            Self::Binary => 0x2,
            Self::Close => 0x8,
            Self::Ping => 0x9,
            Self::Pong => 0xA,
        }
    }

    fn is_control(self) -> bool {
        matches!(self, Self::Close | Self::Ping | Self::Pong)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebSocketFrame {
    pub fin: bool,
    pub opcode: WebSocketOpcode,
    pub payload: Vec<u8>,
}

impl WebSocketFrame {
    pub fn text(value: impl Into<String>) -> Self {
        Self {
            fin: true,
            opcode: WebSocketOpcode::Text,
            payload: value.into().into_bytes(),
        }
    }

    pub fn binary(value: impl Into<Vec<u8>>) -> Self {
        Self {
            fin: true,
            opcode: WebSocketOpcode::Binary,
            payload: value.into(),
        }
    }

    pub fn ping(value: impl Into<Vec<u8>>) -> Self {
        Self {
            fin: true,
            opcode: WebSocketOpcode::Ping,
            payload: value.into(),
        }
    }

    pub fn pong(value: impl Into<Vec<u8>>) -> Self {
        Self {
            fin: true,
            opcode: WebSocketOpcode::Pong,
            payload: value.into(),
        }
    }

    pub fn close(code: u16, reason: &str) -> Self {
        let mut payload = code.to_be_bytes().to_vec();
        payload.extend_from_slice(reason.as_bytes());
        Self {
            fin: true,
            opcode: WebSocketOpcode::Close,
            payload,
        }
    }

    pub fn text_value(&self) -> Result<&str, WebSocketError> {
        if self.opcode != WebSocketOpcode::Text {
            return Err(WebSocketError::Protocol(
                "frame is not a text message".to_string(),
            ));
        }
        std::str::from_utf8(&self.payload)
            .map_err(|_| WebSocketError::Protocol("text payload is not valid UTF-8".to_string()))
    }
}

#[derive(Debug)]
pub enum WebSocketError {
    Io(io::Error),
    BadRequest(String),
    Protocol(String),
    MessageTooLarge { size: u64, limit: usize },
    Closed,
}

impl std::fmt::Display for WebSocketError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "WebSocket I/O error: {error}"),
            Self::BadRequest(message) => write!(f, "invalid WebSocket upgrade: {message}"),
            Self::Protocol(message) => write!(f, "WebSocket protocol error: {message}"),
            Self::MessageTooLarge { size, limit } => {
                write!(f, "WebSocket message size {size} exceeds limit {limit}")
            }
            Self::Closed => write!(f, "WebSocket connection is closed"),
        }
    }
}

impl std::error::Error for WebSocketError {}

impl From<io::Error> for WebSocketError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebSocketUpgrade {
    pub key: String,
    pub protocol: Option<String>,
}

impl WebSocketUpgrade {
    pub fn from_headers(headers: &HashMap<String, String>) -> Result<Self, WebSocketError> {
        let get = |name: &str| {
            headers
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, value)| value.trim())
        };

        let upgrade = get("upgrade")
            .ok_or_else(|| WebSocketError::BadRequest("missing Upgrade header".to_string()))?;
        if !upgrade.eq_ignore_ascii_case("websocket") {
            return Err(WebSocketError::BadRequest(
                "Upgrade header must be websocket".to_string(),
            ));
        }

        let connection = get("connection")
            .ok_or_else(|| WebSocketError::BadRequest("missing Connection header".to_string()))?;
        if !connection
            .split(',')
            .any(|token| token.trim().eq_ignore_ascii_case("upgrade"))
        {
            return Err(WebSocketError::BadRequest(
                "Connection header must contain Upgrade".to_string(),
            ));
        }

        let version = get("sec-websocket-version").ok_or_else(|| {
            WebSocketError::BadRequest("missing Sec-WebSocket-Version header".to_string())
        })?;
        if version != "13" {
            return Err(WebSocketError::BadRequest(
                "only WebSocket version 13 is supported".to_string(),
            ));
        }

        let key = get("sec-websocket-key")
            .ok_or_else(|| {
                WebSocketError::BadRequest("missing Sec-WebSocket-Key header".to_string())
            })?
            .to_string();
        let decoded = STANDARD
            .decode(&key)
            .map_err(|_| WebSocketError::BadRequest("invalid Sec-WebSocket-Key".to_string()))?;
        if decoded.len() != 16 {
            return Err(WebSocketError::BadRequest(
                "Sec-WebSocket-Key must decode to 16 bytes".to_string(),
            ));
        }

        let protocol = get("sec-websocket-protocol")
            .and_then(|value| value.split(',').next())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        Ok(Self { key, protocol })
    }

    pub fn accept_key(&self) -> String {
        websocket_accept_key(&self.key)
    }

    pub fn switching_protocols_response(&self) -> String {
        let mut response = format!(
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n",
            self.accept_key()
        );
        if let Some(protocol) = &self.protocol {
            response.push_str(&format!("Sec-WebSocket-Protocol: {protocol}\r\n"));
        }
        response.push_str("\r\n");
        response
    }
}

pub fn websocket_accept_key(client_key: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(client_key.trim().as_bytes());
    hasher.update(WEBSOCKET_GUID.as_bytes());
    STANDARD.encode(hasher.finalize())
}

pub struct WebSocketConnection<S> {
    stream: S,
    max_message_size: usize,
    fragmented_opcode: Option<WebSocketOpcode>,
    fragmented_payload: Vec<u8>,
    closed: bool,
}

impl<S: Read + Write> WebSocketConnection<S> {
    pub fn new(stream: S) -> Self {
        Self::with_max_message_size(stream, DEFAULT_MAX_MESSAGE_SIZE)
    }

    pub fn with_max_message_size(stream: S, max_message_size: usize) -> Self {
        Self {
            stream,
            max_message_size,
            fragmented_opcode: None,
            fragmented_payload: Vec::new(),
            closed: false,
        }
    }

    pub fn into_inner(self) -> S {
        self.stream
    }

    pub fn send_text(&mut self, value: &str) -> Result<(), WebSocketError> {
        self.write_frame(&WebSocketFrame::text(value))
    }

    pub fn send_binary(&mut self, value: &[u8]) -> Result<(), WebSocketError> {
        self.write_frame(&WebSocketFrame::binary(value.to_vec()))
    }

    pub fn send_ping(&mut self, value: &[u8]) -> Result<(), WebSocketError> {
        self.write_frame(&WebSocketFrame::ping(value.to_vec()))
    }

    pub fn close(&mut self, code: u16, reason: &str) -> Result<(), WebSocketError> {
        if self.closed {
            return Ok(());
        }
        self.write_frame(&WebSocketFrame::close(code, reason))?;
        self.closed = true;
        Ok(())
    }

    pub fn read_message(&mut self) -> Result<WebSocketFrame, WebSocketError> {
        if self.closed {
            return Err(WebSocketError::Closed);
        }

        loop {
            let frame = self.read_frame()?;
            match frame.opcode {
                WebSocketOpcode::Ping => {
                    self.write_frame(&WebSocketFrame::pong(frame.payload))?;
                }
                WebSocketOpcode::Pong => continue,
                WebSocketOpcode::Close => {
                    if !self.closed {
                        self.write_frame(&frame)?;
                    }
                    self.closed = true;
                    return Ok(frame);
                }
                WebSocketOpcode::Continuation => {
                    let opcode = self.fragmented_opcode.ok_or_else(|| {
                        WebSocketError::Protocol(
                            "continuation frame without an active fragmented message".to_string(),
                        )
                    })?;
                    self.extend_fragment(&frame.payload)?;
                    if frame.fin {
                        self.fragmented_opcode = None;
                        return Ok(WebSocketFrame {
                            fin: true,
                            opcode,
                            payload: std::mem::take(&mut self.fragmented_payload),
                        });
                    }
                }
                WebSocketOpcode::Text | WebSocketOpcode::Binary => {
                    if self.fragmented_opcode.is_some() {
                        return Err(WebSocketError::Protocol(
                            "new data frame received during fragmented message".to_string(),
                        ));
                    }
                    if frame.fin {
                        if frame.opcode == WebSocketOpcode::Text {
                            std::str::from_utf8(&frame.payload).map_err(|_| {
                                WebSocketError::Protocol(
                                    "text payload is not valid UTF-8".to_string(),
                                )
                            })?;
                        }
                        return Ok(frame);
                    }
                    self.fragmented_opcode = Some(frame.opcode);
                    self.fragmented_payload.clear();
                    self.extend_fragment(&frame.payload)?;
                }
            }
        }
    }

    pub fn read_frame(&mut self) -> Result<WebSocketFrame, WebSocketError> {
        let mut header = [0_u8; 2];
        self.stream.read_exact(&mut header)?;

        let fin = header[0] & 0x80 != 0;
        if header[0] & 0x70 != 0 {
            return Err(WebSocketError::Protocol(
                "RSV bits require a negotiated extension".to_string(),
            ));
        }
        let opcode = WebSocketOpcode::from_wire(header[0] & 0x0f)?;
        let masked = header[1] & 0x80 != 0;
        if !masked {
            return Err(WebSocketError::Protocol(
                "client-to-server frames must be masked".to_string(),
            ));
        }

        let mut payload_length = u64::from(header[1] & 0x7f);
        if payload_length == 126 {
            let mut bytes = [0_u8; 2];
            self.stream.read_exact(&mut bytes)?;
            payload_length = u64::from(u16::from_be_bytes(bytes));
        } else if payload_length == 127 {
            let mut bytes = [0_u8; 8];
            self.stream.read_exact(&mut bytes)?;
            if bytes[0] & 0x80 != 0 {
                return Err(WebSocketError::Protocol(
                    "invalid 64-bit payload length".to_string(),
                ));
            }
            payload_length = u64::from_be_bytes(bytes);
        }

        if opcode.is_control() {
            if !fin {
                return Err(WebSocketError::Protocol(
                    "control frames must not be fragmented".to_string(),
                ));
            }
            if payload_length > 125 {
                return Err(WebSocketError::Protocol(
                    "control frame payload exceeds 125 bytes".to_string(),
                ));
            }
        }

        if payload_length > self.max_message_size as u64 {
            return Err(WebSocketError::MessageTooLarge {
                size: payload_length,
                limit: self.max_message_size,
            });
        }

        let mut mask = [0_u8; 4];
        self.stream.read_exact(&mut mask)?;
        let payload_len = usize::try_from(payload_length).map_err(|_| {
            WebSocketError::MessageTooLarge {
                size: payload_length,
                limit: self.max_message_size,
            }
        })?;
        let mut payload = vec![0_u8; payload_len];
        self.stream.read_exact(&mut payload)?;
        for (index, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[index % 4];
        }

        Ok(WebSocketFrame {
            fin,
            opcode,
            payload,
        })
    }

    pub fn write_frame(&mut self, frame: &WebSocketFrame) -> Result<(), WebSocketError> {
        if self.closed && frame.opcode != WebSocketOpcode::Close {
            return Err(WebSocketError::Closed);
        }
        if frame.opcode.is_control() {
            if !frame.fin {
                return Err(WebSocketError::Protocol(
                    "control frames must not be fragmented".to_string(),
                ));
            }
            if frame.payload.len() > 125 {
                return Err(WebSocketError::Protocol(
                    "control frame payload exceeds 125 bytes".to_string(),
                ));
            }
        }
        if frame.payload.len() > self.max_message_size {
            return Err(WebSocketError::MessageTooLarge {
                size: frame.payload.len() as u64,
                limit: self.max_message_size,
            });
        }

        let mut header = vec![if frame.fin { 0x80 } else { 0x00 } | frame.opcode.to_wire()];
        let length = frame.payload.len();
        match length {
            0..=125 => header.push(length as u8),
            126..=65_535 => {
                header.push(126);
                header.extend_from_slice(&(length as u16).to_be_bytes());
            }
            _ => {
                header.push(127);
                header.extend_from_slice(&(length as u64).to_be_bytes());
            }
        }
        self.stream.write_all(&header)?;
        self.stream.write_all(&frame.payload)?;
        self.stream.flush()?;
        Ok(())
    }

    fn extend_fragment(&mut self, payload: &[u8]) -> Result<(), WebSocketError> {
        let combined = self.fragmented_payload.len().saturating_add(payload.len());
        if combined > self.max_message_size {
            return Err(WebSocketError::MessageTooLarge {
                size: combined as u64,
                limit: self.max_message_size,
            });
        }
        self.fragmented_payload.extend_from_slice(payload);
        Ok(())
    }
}

pub fn accept_websocket<S: Read + Write>(
    mut stream: S,
    headers: &HashMap<String, String>,
) -> Result<WebSocketConnection<S>, WebSocketError> {
    let upgrade = WebSocketUpgrade::from_headers(headers)?;
    stream.write_all(upgrade.switching_protocols_response().as_bytes())?;
    stream.flush()?;
    Ok(WebSocketConnection::new(stream))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn upgrade_headers() -> HashMap<String, String> {
        HashMap::from([
            ("Upgrade".to_string(), "websocket".to_string()),
            ("Connection".to_string(), "keep-alive, Upgrade".to_string()),
            ("Sec-WebSocket-Version".to_string(), "13".to_string()),
            (
                "Sec-WebSocket-Key".to_string(),
                "dGhlIHNhbXBsZSBub25jZQ==".to_string(),
            ),
        ])
    }

    fn masked_frame(opcode: u8, fin: bool, payload: &[u8]) -> Vec<u8> {
        let mask = [0x37, 0xfa, 0x21, 0x3d];
        let mut bytes = vec![(if fin { 0x80 } else { 0 }) | opcode];
        match payload.len() {
            0..=125 => bytes.push(0x80 | payload.len() as u8),
            126..=65_535 => {
                bytes.push(0x80 | 126);
                bytes.extend_from_slice(&(payload.len() as u16).to_be_bytes());
            }
            _ => {
                bytes.push(0x80 | 127);
                bytes.extend_from_slice(&(payload.len() as u64).to_be_bytes());
            }
        }
        bytes.extend_from_slice(&mask);
        bytes.extend(payload.iter().enumerate().map(|(i, byte)| byte ^ mask[i % 4]));
        bytes
    }

    #[test]
    fn computes_rfc6455_accept_key() {
        assert_eq!(
            websocket_accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    #[test]
    fn validates_upgrade_headers_and_builds_response() {
        let upgrade = WebSocketUpgrade::from_headers(&upgrade_headers()).unwrap();
        let response = upgrade.switching_protocols_response();
        assert!(response.starts_with("HTTP/1.1 101 Switching Protocols\r\n"));
        assert!(response.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo="));
    }

    #[test]
    fn rejects_invalid_version_and_key() {
        let mut headers = upgrade_headers();
        headers.insert("Sec-WebSocket-Version".to_string(), "12".to_string());
        assert!(WebSocketUpgrade::from_headers(&headers).is_err());

        let mut headers = upgrade_headers();
        headers.insert("Sec-WebSocket-Key".to_string(), "invalid".to_string());
        assert!(WebSocketUpgrade::from_headers(&headers).is_err());
    }

    #[test]
    fn decodes_masked_text_frame() {
        let bytes = masked_frame(0x1, true, b"hello");
        let mut connection = WebSocketConnection::new(Cursor::new(bytes));
        let frame = connection.read_message().unwrap();
        assert_eq!(frame.opcode, WebSocketOpcode::Text);
        assert_eq!(frame.text_value().unwrap(), "hello");
    }

    #[test]
    fn assembles_fragmented_text_message() {
        let mut bytes = masked_frame(0x1, false, b"hel");
        bytes.extend(masked_frame(0x0, true, b"lo"));
        let mut connection = WebSocketConnection::new(Cursor::new(bytes));
        let frame = connection.read_message().unwrap();
        assert_eq!(frame.text_value().unwrap(), "hello");
    }

    #[test]
    fn rejects_unmasked_client_frame() {
        let bytes = vec![0x81, 0x02, b'h', b'i'];
        let mut connection = WebSocketConnection::new(Cursor::new(bytes));
        assert!(matches!(
            connection.read_frame(),
            Err(WebSocketError::Protocol(_))
        ));
    }

    #[test]
    fn writes_unmasked_server_frame() {
        let cursor = Cursor::new(Vec::<u8>::new());
        let mut connection = WebSocketConnection::new(cursor);
        connection.send_text("hello").unwrap();
        let bytes = connection.into_inner().into_inner();
        assert_eq!(&bytes[..2], &[0x81, 0x05]);
        assert_eq!(&bytes[2..], b"hello");
    }

    #[test]
    fn enforces_payload_limit() {
        let bytes = masked_frame(0x2, true, &[0; 8]);
        let mut connection = WebSocketConnection::with_max_message_size(Cursor::new(bytes), 4);
        assert!(matches!(
            connection.read_frame(),
            Err(WebSocketError::MessageTooLarge { .. })
        ));
    }

    #[test]
    fn ping_is_answered_with_pong_before_next_message() {
        let mut bytes = masked_frame(0x9, true, b"alive");
        bytes.extend(masked_frame(0x1, true, b"ready"));
        let mut connection = WebSocketConnection::new(Cursor::new(bytes));
        let frame = connection.read_message().unwrap();
        assert_eq!(frame.text_value().unwrap(), "ready");
    }
}
