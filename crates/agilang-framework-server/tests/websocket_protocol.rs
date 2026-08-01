#[path = "../src/websocket.rs"]
mod websocket;

use agilang_framework_routing::Router;
use agilang_framework_server::handle_client;
use agilang_framework_view::ViewEngine;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
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

fn masked_close_frame() -> Vec<u8> {
    let mask = [0x55, 0x66, 0x77, 0x88];
    let payload = 1000u16.to_be_bytes();
    let mut frame = vec![0x88, 0x80 | payload.len() as u8];
    frame.extend_from_slice(&mask);
    frame.extend(
        payload
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ mask[index % 4]),
    );
    frame
}

fn unique_temp_dir(name: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("agilang_websocket_test_{name}_{stamp}"))
}

fn websocket_upgrade_request(path: &str) -> Vec<u8> {
    format!(
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n"
    )
    .into_bytes()
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

#[test]
fn handle_client_upgrades_declared_websocket_route() {
    let root = unique_temp_dir("declared_route");
    let cleanup_root = root.clone();
    fs::create_dir_all(root.join("resources/views")).unwrap();
    let engine = ViewEngine::new(root.join("resources/views"));
    let mut router = Router::new();
    router.add_websocket("/ws".to_string(), "ChatController", "echo");

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        handle_client(stream, &router, &engine, &root).unwrap();
    });

    let mut client = TcpStream::connect(address).unwrap();
    client.write_all(&websocket_upgrade_request("/ws")).unwrap();

    let mut response = Vec::new();
    let mut byte = [0_u8; 1];
    while !response.ends_with(b"\r\n\r\n") {
        client.read_exact(&mut byte).unwrap();
        response.push(byte[0]);
    }
    let response = String::from_utf8(response).unwrap();
    assert!(response.starts_with("HTTP/1.1 101 Switching Protocols"));

    client.write_all(&masked_text_frame("route aware")).unwrap();
    let mut header = [0_u8; 2];
    client.read_exact(&mut header).unwrap();
    assert_eq!(header, [0x81, 11]);
    let mut payload = vec![0_u8; 11];
    client.read_exact(&mut payload).unwrap();
    assert_eq!(payload, b"route aware");

    client.write_all(&masked_close_frame()).unwrap();

    server.join().unwrap();
    fs::remove_dir_all(cleanup_root).ok();
}

#[test]
fn handle_client_rejects_undeclared_websocket_route() {
    let root = unique_temp_dir("missing_route");
    let cleanup_root = root.clone();
    fs::create_dir_all(root.join("resources/views")).unwrap();
    let engine = ViewEngine::new(root.join("resources/views"));
    let router = Router::new();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        handle_client(stream, &router, &engine, &root).unwrap();
    });

    let mut client = TcpStream::connect(address).unwrap();
    client.write_all(&websocket_upgrade_request("/ws")).unwrap();
    let mut response = Vec::new();
    client.read_to_end(&mut response).unwrap();
    let response = String::from_utf8(response).unwrap();
    assert!(response.starts_with("HTTP/1.1 404 Not Found"));
    assert!(response.contains("WebSocket route not defined."));

    server.join().unwrap();
    fs::remove_dir_all(cleanup_root).ok();
}
