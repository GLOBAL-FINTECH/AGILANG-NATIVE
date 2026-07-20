use agilang_framework_http::{HttpMethod, Request};
use agilang_framework_routing::Router;
use agilang_framework_view::ViewEngine;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::time::Instant;

pub enum ControllerResult {
    Render(String, HashMap<String, String>),
    Html(String),
    Json(String),
}

pub fn percent_decode(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            let mut hex = String::new();
            if let Some(h1) = chars.next() {
                hex.push(h1);
            }
            if let Some(h2) = chars.next() {
                hex.push(h2);
            }
            if let Ok(val) = u8::from_str_radix(&hex, 16) {
                result.push(val as char);
            } else {
                result.push('%');
                result.push_str(&hex);
            }
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }
    result
}

pub fn parse_query(query_str: &str) -> HashMap<String, Vec<String>> {
    let mut params = HashMap::new();
    for pair in query_str.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, val) = match pair.find('=') {
            Some(pos) => {
                let k = percent_decode(&pair[..pos]);
                let v = percent_decode(&pair[pos + 1..]);
                (k, v)
            }
            None => (percent_decode(pair), String::new()),
        };
        params.entry(key).or_insert_with(Vec::new).push(val);
    }
    params
}

pub fn parse_http_request(stream: &mut TcpStream) -> Result<Request, String> {
    let mut reader = BufReader::new(stream);
    let mut first_line = String::new();
    reader
        .read_line(&mut first_line)
        .map_err(|e| e.to_string())?;

    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 3 {
        return Err("Malformed HTTP request line".to_string());
    }

    let method = match parts[0] {
        "GET" => HttpMethod::Get,
        "POST" => HttpMethod::Post,
        "PUT" => HttpMethod::Put,
        "DELETE" => HttpMethod::Delete,
        "OPTIONS" => HttpMethod::Options,
        "HEAD" => HttpMethod::Head,
        "PATCH" => HttpMethod::Patch,
        _ => return Err(format!("Unsupported method: {}", parts[0])),
    };

    let full_path = parts[1];
    let (path_str, query_str) = match full_path.find('?') {
        Some(pos) => (&full_path[..pos], &full_path[pos + 1..]),
        None => (full_path, ""),
    };

    let path = percent_decode(path_str);
    let query = parse_query(query_str);

    let mut headers = HashMap::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| e.to_string())?;
        let line = line.trim();
        if line.is_empty() {
            break;
        }
        if let Some(pos) = line.find(':') {
            let key = line[..pos].trim().to_lowercase();
            let val = line[pos + 1..].trim().to_string();
            headers.insert(key, val);
        }
    }

    let mut body = Vec::new();
    if let Some(len_str) = headers.get("content-length") {
        if let Ok(len) = len_str.parse::<usize>() {
            body.resize(len, 0);
            reader.read_exact(&mut body).map_err(|e| e.to_string())?;
        }
    }

    Ok(Request {
        method,
        path,
        query,
        headers,
        body,
        remote_addr: reader.get_ref().peer_addr().ok(),
    })
}

pub fn evaluate_controller_action(
    controller_path: &Path,
    action: &str,
) -> Result<ControllerResult, String> {
    let content = std::fs::read_to_string(controller_path)
        .map_err(|e| format!("Failed to read controller: {}", e))?;

    let func_marker = format!("fn {}", action);
    let func_pos = content
        .find(&func_marker)
        .ok_or_else(|| format!("Action `{}` not found in controller", action))?;

    let func_body = &content[func_pos..];

    let return_pos = func_body
        .find("return ")
        .ok_or_else(|| format!("No return statement found in action `{}`", action))?;

    let return_str = &func_body[return_pos + 7..];
    let mut expr = String::new();
    let mut paren_depth = 0;
    let mut brace_depth = 0;
    let mut bracket_depth = 0;
    let mut in_double_quote = false;
    let mut in_single_quote = false;
    let mut escaped = false;

    for c in return_str.chars() {
        if escaped {
            expr.push(c);
            escaped = false;
            continue;
        }

        match c {
            '\\' => {
                expr.push(c);
                escaped = true;
            }
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
                expr.push(c);
            }
            '\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
                expr.push(c);
            }
            _ => {
                if !in_double_quote && !in_single_quote {
                    match c {
                        '(' => paren_depth += 1,
                        ')' => {
                            paren_depth -= 1;
                            if paren_depth < 0 {
                                break;
                            }
                        }
                        '{' => brace_depth += 1,
                        '}' => {
                            brace_depth -= 1;
                        }
                        '[' => bracket_depth += 1,
                        ']' => bracket_depth -= 1,
                        '\n' if paren_depth == 0 && brace_depth == 0 && bracket_depth == 0 => {
                            break;
                        }
                        _ => {}
                    }
                }
                expr.push(c);
            }
        }
    }
    let expr = expr.trim();

    if expr.starts_with("View.render") {
        let view_start = expr
            .find('"')
            .or_else(|| expr.find('\''))
            .ok_or("Malformed View.render statement")?;
        let view_sub = &expr[view_start + 1..];
        let view_end = view_sub
            .find('"')
            .or_else(|| view_sub.find('\''))
            .ok_or("Malformed View.render view name")?;
        let view_name = &view_sub[..view_end];

        let mut data = HashMap::new();
        if let Some(dict_start) = expr.find('{') {
            if let Some(dict_end) = expr.rfind('}') {
                let dict_content = &expr[dict_start + 1..dict_end];
                for pair in dict_content.split(',') {
                    let pair = pair.trim();
                    if pair.is_empty() {
                        continue;
                    }
                    if let Some(colon_pos) = pair.find(':') {
                        let key = pair[..colon_pos]
                            .trim()
                            .trim_matches('"')
                            .trim_matches('\'');
                        let val = pair[colon_pos + 1..]
                            .trim()
                            .trim_matches('"')
                            .trim_matches('\'');
                        data.insert(key.to_string(), val.to_string());
                    }
                }
            }
        }
        Ok(ControllerResult::Render(view_name.to_string(), data))
    } else if expr.starts_with("Response.html") {
        let html_start = expr
            .find('"')
            .or_else(|| expr.find('\''))
            .ok_or("Malformed Response.html statement")?;
        let html_sub = &expr[html_start + 1..];
        let html_end = html_sub
            .rfind('"')
            .or_else(|| html_sub.rfind('\''))
            .ok_or("Malformed Response.html body")?;
        let html_body = &html_sub[..html_end];
        Ok(ControllerResult::Html(
            html_body.replace("\\n", "\n").replace("\\r", "\r"),
        ))
    } else if expr.starts_with("Response.json") {
        let mut data = HashMap::new();
        if let Some(dict_start) = expr.find('{') {
            if let Some(dict_end) = expr.rfind('}') {
                let dict_content = &expr[dict_start + 1..dict_end];
                for pair in dict_content.split(',') {
                    let pair = pair.trim();
                    if pair.is_empty() {
                        continue;
                    }
                    if let Some(colon_pos) = pair.find(':') {
                        let key = pair[..colon_pos]
                            .trim()
                            .trim_matches('"')
                            .trim_matches('\'');
                        let val = pair[colon_pos + 1..]
                            .trim()
                            .trim_matches('"')
                            .trim_matches('\'');
                        data.insert(key.to_string(), val.to_string());
                    }
                }
            }
        }
        let mut json = String::new();
        json.push_str("{\n");
        let keys: Vec<&String> = data.keys().collect();
        for (idx, k) in keys.iter().enumerate() {
            let v = data.get(*k).unwrap();
            json.push_str(&format!("  \"{}\": \"{}\"", k, v));
            if idx < keys.len() - 1 {
                json.push_str(",\n");
            } else {
                json.push('\n');
            }
        }
        json.push_str("}\n");
        Ok(ControllerResult::Json(json))
    } else {
        Err(format!("Unsupported return expression: {}", expr))
    }
}

pub fn bind_with_fallback(
    host: IpAddr,
    requested_port: u16,
    allow_fallback: bool,
) -> std::io::Result<(TcpListener, u16)> {
    let first = SocketAddr::new(host, requested_port);

    match TcpListener::bind(first) {
        Ok(listener) => {
            start_stun_server();
            Ok((listener, requested_port))
        }
        Err(error) if allow_fallback && error.kind() == std::io::ErrorKind::AddrInUse => {
            for port in requested_port.saturating_add(1)..=requested_port.saturating_add(100) {
                let candidate = SocketAddr::new(host, port);
                if let Ok(listener) = TcpListener::bind(candidate) {
                    start_stun_server();
                    return Ok((listener, port));
                }
            }
            Err(error)
        }
        Err(error) => Err(error),
    }
}

pub fn handle_client(
    mut stream: TcpStream,
    router: &Router,
    view_engine: &ViewEngine,
    project_root: &Path,
) -> std::io::Result<()> {
    let start_time = Instant::now();
    let request_res = parse_http_request(&mut stream);

    let mut req_method = HttpMethod::Get;
    let mut req_path = String::new();

    let (status_code, body, content_type) = match request_res {
        Err(ref e) => (
            500,
            format!("<h1>500 Internal Server Error</h1><pre>{}</pre>", e),
            "text/html; charset=utf-8",
        ),
        Ok(ref req) => {
            req_method = req.method.clone();
            req_path = req.path.clone();
            let path = req.path();
            let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S");

            let is_websocket = req
                .headers
                .get("upgrade")
                .or_else(|| req.headers.get("Upgrade"))
                .map(|v| v.to_lowercase() == "websocket")
                .unwrap_or(false);

            if is_websocket {
                if let Some(key) = req
                    .headers
                    .get("Sec-WebSocket-Key")
                    .or_else(|| req.headers.get("sec-websocket-key"))
                {
                    let accept_key = handle_websocket_upgrade(key);
                    let handshake_response = format!(
                        "HTTP/1.1 101 Switching Protocols\r\n\
                         Upgrade: websocket\r\n\
                         Connection: Upgrade\r\n\
                         Sec-WebSocket-Accept: {}\r\n\r\n",
                        accept_key
                    );
                    let _ = stream.write_all(handshake_response.as_bytes());
                    let _ = stream.flush();

                    if let Err(e) = run_websocket_echo_loop(stream) {
                        eprintln!("websocket error: {}", e);
                    }
                    return Ok(());
                }
            }

            // Serves static files from public/ first
            let public_path = project_root
                .join("public")
                .join(path.trim_start_matches('/'));
            if public_path.is_file() && !path.ends_with('/') {
                if let Ok(content) = std::fs::read(public_path) {
                    let ext = Path::new(path)
                        .extension()
                        .and_then(|s| s.to_str())
                        .unwrap_or("");
                    let mime = match ext {
                        "css" => "text/css",
                        "js" => "application/javascript",
                        "svg" => "image/svg+xml",
                        "html" => "text/html",
                        _ => "application/octet-stream",
                    };

                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        mime,
                        content.len()
                    );
                    stream.write_all(response.as_bytes()).ok();
                    stream.write_all(&content).ok();
                    stream.flush().ok();
                    println!(
                        "{} {:?} {} 200 {}ms",
                        now,
                        req.method,
                        path,
                        start_time.elapsed().as_millis()
                    );
                    return Ok(());
                }
            }

            // Route matching
            match router.match_route(&req.method, path) {
                None => (
                    404,
                    "<h1>404 Not Found</h1><p>Route not defined.</p>".to_string(),
                    "text/html; charset=utf-8",
                ),
                Some(route) => {
                    let controllers_dir = project_root.join("app/Controllers");
                    let controller_file = find_controller_file(&controllers_dir, &route.controller);

                    if let Some(file) = controller_file {
                        match evaluate_controller_action(&file, &route.action) {
                            Err(e) => (
                                500,
                                format!("<h1>500 Internal Server Error</h1><pre>{}</pre>", e),
                                "text/html; charset=utf-8",
                            ),
                            Ok(ControllerResult::Html(html)) => {
                                (200, html, "text/html; charset=utf-8")
                            }
                            Ok(ControllerResult::Json(json)) => (200, json, "application/json"),
                            Ok(ControllerResult::Render(view, data)) => {
                                match view_engine.render(&view, &data) {
                                    Ok(html) => (200, html, "text/html; charset=utf-8"),
                                    Err(e) => (
                                        500,
                                        format!(
                                            "<h1>500 Internal Server Error</h1><pre>{}</pre>",
                                            e
                                        ),
                                        "text/html; charset=utf-8",
                                    ),
                                }
                            }
                        }
                    } else {
                        (
                            500,
                            format!("<h1>500 Internal Server Error</h1><p>Controller `{}` not found.</p>", route.controller),
                            "text/html; charset=utf-8",
                        )
                    }
                }
            }
        }
    };

    let status_line = match status_code {
        200 => "HTTP/1.1 200 OK",
        404 => "HTTP/1.1 404 Not Found",
        500 => "HTTP/1.1 500 Internal Server Error",
        _ => "HTTP/1.1 500 Internal Server Error",
    };

    let response = format!(
        "{}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status_line,
        content_type,
        body.len(),
        body
    );

    stream.write_all(response.as_bytes())?;
    stream.flush()?;

    // Log the request
    if request_res.is_ok() {
        let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S");
        println!(
            "{} {:?} {} {} {}ms",
            now,
            req_method,
            req_path,
            status_code,
            start_time.elapsed().as_millis()
        );
    }

    Ok(())
}

fn find_controller_file(dir: &Path, controller_name: &str) -> Option<std::path::PathBuf> {
    let target_filename = format!("{}.agi", controller_name.replace('.', "/"));
    let direct = dir.join(&target_filename);
    if direct.is_file() {
        return Some(direct);
    }

    let leaf_name = if let Some(last_slash) = target_filename.rfind('/') {
        &target_filename[last_slash + 1..]
    } else {
        &target_filename
    };

    find_file_recursive(dir, leaf_name)
}

fn find_file_recursive(dir: &Path, filename: &str) -> Option<std::path::PathBuf> {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(found) = find_file_recursive(&path, filename) {
                    return Some(found);
                }
            } else if path.file_name().and_then(|s| s.to_str()) == Some(filename) {
                return Some(path);
            }
        }
    }
    None
}

pub fn start_stun_server() {
    std::thread::spawn(|| {
        let socket = match std::net::UdpSocket::bind("0.0.0.0:3478") {
            Ok(s) => s,
            Err(_) => {
                // If port 3478 is occupied (e.g. by another test/server), fail silently in dev
                return;
            }
        };

        let mut buf = [0u8; 1024];
        loop {
            if let Ok((amt, src)) = socket.recv_from(&mut buf) {
                if amt < 20 {
                    continue;
                }

                let msg_type = u16::from_be_bytes([buf[0], buf[1]]);
                let magic_cookie = &buf[4..8];
                let transaction_id = &buf[8..20];

                if msg_type == 0x0001 {
                    let mut response = Vec::new();
                    response.extend_from_slice(&0x0101u16.to_be_bytes());
                    response.extend_from_slice(&12u16.to_be_bytes());
                    response.extend_from_slice(magic_cookie);
                    response.extend_from_slice(transaction_id);

                    // XOR-MAPPED-ADDRESS Attribute (Type: 0x0020, Length: 8)
                    response.extend_from_slice(&0x0020u16.to_be_bytes());
                    response.extend_from_slice(&8u16.to_be_bytes());
                    response.push(0x00);
                    response.push(0x01); // IPv4 Family

                    let port = src.port();
                    let xor_port = port ^ 0x2112;
                    response.extend_from_slice(&xor_port.to_be_bytes());

                    if let std::net::SocketAddr::V4(addr) = src {
                        let ip_octets = addr.ip().octets();
                        let mut xor_ip = [0u8; 4];
                        xor_ip[0] = ip_octets[0] ^ 0x21;
                        xor_ip[1] = ip_octets[1] ^ 0x12;
                        xor_ip[2] = ip_octets[2] ^ 0xA4;
                        xor_ip[3] = ip_octets[3] ^ 0x42;
                        response.extend_from_slice(&xor_ip);
                    } else {
                        response.extend_from_slice(&[127 ^ 0x21, 0x12, 0xA4, 1 ^ 0x42]);
                    }

                    let _ = socket.send_to(&response, src);
                }
            }
        }
    });
}

fn handle_websocket_upgrade(key: &str) -> String {
    let mut concatenated = key.to_string();
    concatenated.push_str("258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    let hashed = sha1_hash(concatenated.as_bytes());
    base64_encode(&hashed)
}

fn base64_encode(input: &[u8]) -> String {
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(input.len().div_ceil(3) * 4);
    let mut i = 0;
    while i < input.len() {
        let chunk = &input[i..std::cmp::min(i + 3, input.len())];
        let mut b = 0u32;
        for (j, val) in chunk.iter().enumerate() {
            b |= (*val as u32) << (16 - j * 8);
        }

        result.push(CHARSET[((b >> 18) & 63) as usize] as char);
        result.push(CHARSET[((b >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARSET[((b >> 6) & 63) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARSET[(b & 63) as usize] as char);
        } else {
            result.push('=');
        }
        i += 3;
    }
    result
}

#[allow(clippy::needless_range_loop)]
fn sha1_hash(data: &[u8]) -> [u8; 20] {
    let mut h0: u32 = 0x67452301;
    let mut h1: u32 = 0xEFCDAB89;
    let mut h2: u32 = 0x98BADCFE;
    let mut h3: u32 = 0x10325476;
    let mut h4: u32 = 0xC3D2E1F0;

    let mut padded = data.to_vec();
    let original_len_bits = (data.len() as u64) * 8;

    padded.push(0x80);

    while (padded.len() % 64) != 56 {
        padded.push(0x00);
    }

    padded.extend_from_slice(&original_len_bits.to_be_bytes());

    for chunk in padded.chunks_exact(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let mut a = h0;
        let mut b = h1;
        let mut c = h2;
        let mut d = h3;
        let mut e = h4;

        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };

            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w[i]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    let mut result = [0u8; 20];
    result[0..4].copy_from_slice(&h0.to_be_bytes());
    result[4..8].copy_from_slice(&h1.to_be_bytes());
    result[8..12].copy_from_slice(&h2.to_be_bytes());
    result[12..16].copy_from_slice(&h3.to_be_bytes());
    result[16..20].copy_from_slice(&h4.to_be_bytes());
    result
}

fn run_websocket_echo_loop(mut stream: TcpStream) -> std::io::Result<()> {
    let mut buf = [0u8; 4096];
    loop {
        stream.read_exact(&mut buf[..2])?;
        let byte0 = buf[0];
        let byte1 = buf[1];

        let opcode = byte0 & 0x0F;
        if opcode == 0x8 {
            break;
        }

        let is_masked = (byte1 & 0x80) != 0;
        let mut payload_len = (byte1 & 0x7F) as usize;

        if payload_len == 126 {
            stream.read_exact(&mut buf[2..4])?;
            payload_len = u16::from_be_bytes([buf[2], buf[3]]) as usize;
        } else if payload_len == 127 {
            stream.read_exact(&mut buf[2..10])?;
            let mut len_bytes = [0u8; 8];
            len_bytes.copy_from_slice(&buf[2..10]);
            payload_len = u64::from_be_bytes(len_bytes) as usize;
        }

        let mut mask_key = [0u8; 4];
        if is_masked {
            stream.read_exact(&mut mask_key)?;
        }

        let mut payload = vec![0u8; payload_len];
        stream.read_exact(&mut payload)?;

        if is_masked {
            for i in 0..payload_len {
                payload[i] ^= mask_key[i % 4];
            }
        }

        if opcode == 0x1 {
            let mut response_frame = Vec::new();
            response_frame.push(0x81);

            if payload_len <= 125 {
                response_frame.push(payload_len as u8);
            } else if payload_len <= 65535 {
                response_frame.push(126);
                response_frame.extend_from_slice(&(payload_len as u16).to_be_bytes());
            } else {
                response_frame.push(127);
                response_frame.extend_from_slice(&(payload_len as u64).to_be_bytes());
            }
            response_frame.extend_from_slice(&payload);
            stream.write_all(&response_frame)?;
            stream.flush()?;
        }
    }
    Ok(())
}

pub struct CertificateGenerator;

impl CertificateGenerator {
    pub fn generate_dev_cert(cert_path: &Path, key_path: &Path) -> std::io::Result<()> {
        if let Some(parent) = cert_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if let Some(parent) = key_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let cert_pem = "-----BEGIN CERTIFICATE-----\nMIIDAzCCAemgAwIBAgIUAGILANGDEV01MA0GCSqGSIb3DQEBCwUAMB4xHDAaBgNV\nBAMMEzEyNy4wLjAuMSBEZXYgQ2VydDAeFw0yNjA3MjAwMDAwMDBaFw0zNjA3MjAw\nMDAwMDBaMB4xHDAaBgNVBAMMEzEyNy4wLjAuMSBEZXYgQ2VydDCCASIwDQYJKoZI\nhvcNAQEBBQADggEPADCCAQoCggEBALV+W5s/\n-----END CERTIFICATE-----\n";
        let key_pem = "-----BEGIN PRIVATE KEY-----\nMIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQC1flo7\n-----END PRIVATE KEY-----\n";

        std::fs::write(cert_path, cert_pem)?;
        std::fs::write(key_path, key_pem)?;
        Ok(())
    }
}
