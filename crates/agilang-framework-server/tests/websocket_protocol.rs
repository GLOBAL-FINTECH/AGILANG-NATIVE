#[path = "../src/websocket.rs"]
mod websocket;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use websocket::{accept_websocket, WebSocketOpcode};

fn read_http_upgrade(stream: &mut TcpStream) -> HashMap<String, String> {
    let mut bytes = Vec::new();
    let mut single = [0_u8; 1];
    while !bytes.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut single).unwrap();
        bytes.push(single[0]);
        assert!(bytes.len() < 16 * 1024, "upgrade request exceeded limit");
    }
    let request = String::from_utf8(bytes).unwrap();
    request
        .split("\r\n")
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_string(), value.trim().to_string()))
        .collect()
}

fn masked_text_frame(value: &str) -> Vec<u8> {
    let payload = value.as_bytes();
    let mask = [0x11, 0x22, 0x33, 0x44];
    assert!(payload.len() <= 125);
    let mut frame = vec![0x81, 0x80 | payload.len() as u8];
    frame.extend_from_slice(&mask);
    frame.extend(
        payload
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ mask[index % 4]),
    );
    frame
}

#[test]
fn websocket_upgrade_and_echo_work_over_tcp() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let headers = read_http_upgrade(&mut stream);
        let mut socket = accept_websocket(stream, &headers).unwrap();
        let message = socket.read_message().unwrap();
        assert_eq!(message.opcode, WebSocketOpcode::Text);
        assert_eq!(message.text_value().unwrap(), "hello AGILANG");
        socket.send_text("hello client").unwrap();
        socket.close(1000, "complete").unwrap();
    });

    let mut client = TcpStream::connect(address).unwrap();
    client
        .write_all(
            b"GET /ws HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n",
        )
        .unwrap();

    let mut response = Vec::new();
    let mut byte = [0_u8; 1];
    while !response.ends_with(b"\r\n\r\n") {
        client.read_exact(&mut byte).unwrap();
        response.push(byte[0]);
    }
    let response = String::from_utf8(response).unwrap();
    assert!(response.starts_with("HTTP/1.1 101 Switching Protocols"));
    assert!(response.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo="));

    client.write_all(&masked_text_frame("hello AGILANG")).unwrap();

    let mut header = [0_u8; 2];
    client.read_exact(&mut header).unwrap();
    assert_eq!(header, [0x81, 12]);
    let mut payload = vec![0_u8; 12];
    client.read_exact(&mut payload).unwrap();
    assert_eq!(payload, b"hello client");

    let mut close_header = [0_u8; 2];
    client.read_exact(&mut close_header).unwrap();
    assert_eq!(close_header[0], 0x88);

    server.join().unwrap();
}
