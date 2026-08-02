mod controller_runtime;
mod framework_manifest;
mod websocket;
pub use framework_manifest::write_framework_manifest;

use agilang_database_agidb::AgiDbConnection;
use agilang_database_driver::{DatabaseConnection, DatabaseRow, DatabaseValue};
use agilang_framework_auth::{Argon2idPasswordHasher, PasswordHasher, SessionToken};
use agilang_framework_http::{HttpMethod, Request, Response};
use agilang_framework_routing::Router;
use agilang_framework_view::ViewEngine;
use agilang_runtime_crypto::sha256;
use agilang_runtime_webrtc::{SignalEnvelope, SignalKind, SignalingHub};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use websocket::{accept_websocket, WebSocketConnection, WebSocketError, WebSocketOpcode};

pub enum ControllerResult {
    Render(String, HashMap<String, String>),
    Html(String),
    Json(u16, String),
}

static AUTH_RATE_LIMITER: OnceLock<Mutex<HashMap<String, Vec<u64>>>> = OnceLock::new();
static ACTIVE_REGISTRATIONS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
static WEBRTC_SIGNALING_HUB: OnceLock<SignalingHub> = OnceLock::new();
static WEBRTC_PEER_REGISTRY: OnceLock<Mutex<HashMap<String, WebRtcPeerRegistration>>> = OnceLock::new();
static WEBRTC_MEETING_REGISTRY: OnceLock<Mutex<HashMap<String, WebRtcMeetingRegistration>>> =
    OnceLock::new();

const SESSION_COOKIE_NAME: &str = "agilang_session";
const SESSION_TTL_SECS: u64 = 7200;
const RATE_LIMIT_WINDOW_SECS: u64 = 60;
const RATE_LIMIT_LOGIN_MAX: usize = 8;
const RATE_LIMIT_REGISTER_MAX: usize = 6;
const MAX_ACTIVE_USER_SESSIONS: usize = 4;

#[derive(Debug, Clone)]
struct AuthSessionRecord {
    token_hash: String,
    user_id: String,
    csrf_secret: String,
    expires_at: u64,
}

#[derive(Debug, Clone)]
struct AuthUserRecord {
    id: String,
    numeric_id: Option<i64>,
    name: String,
    email: String,
    password_hash: String,
    role: String,
}

#[derive(Debug, Clone)]
struct WebRtcPeerRegistration {
    session_token_hash: String,
    user_id: String,
    email: String,
    meeting_id: Option<String>,
}

#[derive(Debug, Clone)]
struct WebRtcMeetingMember {
    peer_id: String,
    user_id: String,
    email: String,
    role: String,
}

#[derive(Debug, Clone, Default)]
struct WebRtcMeetingRegistration {
    host_peer_id: Option<String>,
    members: BTreeMap<String, WebRtcMeetingMember>,
}

struct RegistrationGuard {
    email: String,
}

impl RegistrationGuard {
    fn acquire(email: &str) -> Self {
        let registrations = ACTIVE_REGISTRATIONS.get_or_init(|| Mutex::new(HashSet::new()));
        loop {
            let mut guard = registrations.lock().unwrap();
            if !guard.contains(email) {
                guard.insert(email.to_string());
                return Self {
                    email: email.to_string(),
                };
            }
            drop(guard);
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for RegistrationGuard {
    fn drop(&mut self) {
        if let Some(registrations) = ACTIVE_REGISTRATIONS.get() {
            registrations.lock().unwrap().remove(&self.email);
        }
    }
}

#[derive(Debug, Clone)]
enum JsonValue {
    String(String),
    Number(String),
    Bool(bool),
    Null,
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
    Unsupported(String),
}

impl JsonValue {
    fn to_json(&self) -> String {
        match self {
            JsonValue::String(value) => format!("\"{}\"", escape_json(value)),
            JsonValue::Number(value) => value.clone(),
            JsonValue::Bool(value) => value.to_string(),
            JsonValue::Null => "null".to_string(),
            JsonValue::Array(values) => {
                let inner = values
                    .iter()
                    .map(JsonValue::to_json)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("[{}]", inner)
            }
            JsonValue::Object(entries) => {
                let inner = entries
                    .iter()
                    .map(|(key, value)| format!("\"{}\": {}", escape_json(key), value.to_json()))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{}}}", inner)
            }
            JsonValue::Unsupported(expr) => {
                format!("{{\"__agilang_unsupported__\": \"{}\"}}", escape_json(expr))
            }
        }
    }
}

fn escape_json(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0)
}

fn hash_session_token(token: &str) -> String {
    sha256(token.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn resolved_auth_db_path(project_root: &Path) -> String {
    let config = agilang_framework_config::Config::load(project_root);
    let configured = config.get("AGIDB_DATABASE", "");
    if !configured.is_empty() {
        let path = Path::new(&configured);
        return if path.is_absolute() {
            path.to_string_lossy().to_string()
        } else {
            project_root.join(path).to_string_lossy().to_string()
        };
    }
    let configured = config.get("DATABASE_PATH", "");
    if !configured.is_empty() {
        let path = Path::new(&configured);
        return if path.is_absolute() {
            path.to_string_lossy().to_string()
        } else {
            project_root.join(path).to_string_lossy().to_string()
        };
    }
    if let Some(configured) = agilang_toml_database_path(project_root) {
        let path = Path::new(&configured);
        return if path.is_absolute() {
            path.to_string_lossy().to_string()
        } else {
            project_root.join(path).to_string_lossy().to_string()
        };
    }
    project_root
        .join("storage/database/main.agidb")
        .to_string_lossy()
        .to_string()
}

fn agilang_toml_database_path(project_root: &Path) -> Option<String> {
    let config_path = project_root.join("agilang.toml");
    let content = std::fs::read_to_string(config_path).ok()?;
    let mut in_database_section = false;
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_database_section = line == "[database]";
            continue;
        }
        if !in_database_section {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            if key.trim() == "path" {
                return Some(value.trim().trim_matches('"').to_string());
            }
        }
    }
    None
}

fn open_auth_db(project_root: &Path) -> Result<AgiDbConnection, String> {
    let path = resolved_auth_db_path(project_root);
    let mut conn = AgiDbConnection::new(path);
    ensure_auth_tables(&mut conn)?;
    repair_legacy_auth_rows(&mut conn)?;
    prune_expired_sessions(&mut conn)?;
    Ok(conn)
}

fn ensure_auth_tables(conn: &mut AgiDbConnection) -> Result<(), String> {
    conn.execute(
        "CREATE TABLE users (id INTEGER, name TEXT, email TEXT, password_hash TEXT, role TEXT, created_at INTEGER, updated_at INTEGER)",
        &[],
    )
    .map_err(|err| err.to_string())?;
    conn.execute(
        "CREATE TABLE sessions (id INTEGER, user_id INTEGER, token TEXT, expires_at INTEGER, created_at INTEGER, csrf_secret TEXT)",
        &[],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

fn repair_legacy_auth_rows(conn: &mut AgiDbConnection) -> Result<(), String> {
    let session_rows = conn
        .query("SELECT * FROM sessions", &[])
        .map_err(|err| err.to_string())?;
    for row in session_rows {
        let Some(session_id) = row_integer(&row, "id") else {
            continue;
        };
        let Some(user_id_text) = row_text(&row, "user_id") else {
            continue;
        };
        let Ok(user_id_value) = user_id_text.parse::<i64>() else {
            continue;
        };
        conn.execute(
            "UPDATE sessions SET user_id = ? WHERE id = ?",
            &[
                DatabaseValue::Integer(user_id_value),
                DatabaseValue::Integer(session_id),
            ],
        )
        .map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn prune_expired_sessions(conn: &mut AgiDbConnection) -> Result<(), String> {
    let now = now_unix() as i64;
    conn.execute(
        "DELETE FROM sessions WHERE expires_at = ?",
        &[DatabaseValue::Integer(now)],
    )
    .ok();
    let rows = conn
        .query("SELECT * FROM sessions", &[])
        .map_err(|err| err.to_string())?;
    for row in rows {
        if let Some(expires_at) = row_integer(&row, "expires_at") {
            if expires_at <= now {
                if let Some(token_hash) = row_text(&row, "token") {
                    conn.execute(
                        "DELETE FROM sessions WHERE token = ?",
                        &[DatabaseValue::Text(token_hash.to_string())],
                    )
                    .ok();
                }
            }
        }
    }
    Ok(())
}

fn parse_cookies(req: &Request) -> HashMap<String, String> {
    req.headers
        .get("cookie")
        .map(|raw| {
            raw.split(';')
                .filter_map(|pair| {
                    let (key, value) = pair.trim().split_once('=')?;
                    Some((key.trim().to_string(), value.trim().to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_form_body(req: &Request) -> HashMap<String, Vec<String>> {
    let content_type = req.headers.get("content-type").cloned().unwrap_or_default();
    if content_type.starts_with("application/x-www-form-urlencoded") {
        let body = String::from_utf8_lossy(&req.body);
        return parse_query(&body);
    }
    HashMap::new()
}

fn row_text<'a>(row: &'a DatabaseRow, key: &str) -> Option<&'a str> {
    match row.get(key) {
        Some(DatabaseValue::Text(value)) => Some(value.as_str()),
        _ => None,
    }
}

fn row_integer(row: &DatabaseRow, key: &str) -> Option<i64> {
    match row.get(key) {
        Some(DatabaseValue::Integer(value)) => Some(*value),
        _ => None,
    }
}

fn load_session_by_cookie(
    conn: &mut AgiDbConnection,
    req: &Request,
) -> Result<Option<AuthSessionRecord>, String> {
    let cookies = parse_cookies(req);
    let Some(raw_token) = cookies.get(SESSION_COOKIE_NAME) else {
        return Ok(None);
    };
    let token_hash = hash_session_token(raw_token);
    let rows = conn
        .query(
            "SELECT * FROM sessions WHERE token = ? LIMIT 1",
            &[DatabaseValue::Text(token_hash.clone())],
        )
        .map_err(|err| err.to_string())?;
    let Some(row) = rows.first() else {
        return Ok(None);
    };
    let expires_at = row_integer(row, "expires_at").unwrap_or_default() as u64;
    if expires_at <= now_unix() {
        conn.execute(
            "DELETE FROM sessions WHERE token = ?",
            &[DatabaseValue::Text(token_hash)],
        )
        .ok();
        return Ok(None);
    }
    Ok(Some(AuthSessionRecord {
        token_hash,
        user_id: row_integer(row, "user_id")
            .map(|value| value.to_string())
            .or_else(|| row_text(row, "user_id").map(|value| value.to_string()))
            .unwrap_or_default(),
        csrf_secret: row_text(row, "csrf_secret").unwrap_or_default().to_string(),
        expires_at,
    }))
}

fn load_user_by_id(
    conn: &mut AgiDbConnection,
    user_id: &str,
) -> Result<Option<AuthUserRecord>, String> {
    if user_id.is_empty() {
        return Ok(None);
    }
    let params = match user_id.parse::<i64>() {
        Ok(value) => vec![DatabaseValue::Integer(value)],
        Err(_) => vec![DatabaseValue::Text(user_id.to_string())],
    };
    let rows = conn
        .query("SELECT * FROM users WHERE id = ? LIMIT 1", &params)
        .map_err(|err| err.to_string())?;
    Ok(rows.first().map(auth_user_from_row))
}

fn auth_user_from_row(row: &DatabaseRow) -> AuthUserRecord {
    let numeric_id = row_integer(row, "id");
    AuthUserRecord {
        id: numeric_id
            .map(|value| value.to_string())
            .or_else(|| row_text(row, "id").map(|value| value.to_string()))
            .unwrap_or_default(),
        numeric_id,
        name: row_text(row, "name").unwrap_or_default().to_string(),
        email: row_text(row, "email").unwrap_or_default().to_string(),
        password_hash: row_text(row, "password_hash")
            .unwrap_or_default()
            .to_string(),
        role: row_text(row, "role").unwrap_or("user").to_string(),
    }
}

fn load_users_by_email(
    conn: &mut AgiDbConnection,
    email: &str,
) -> Result<Vec<AuthUserRecord>, String> {
    let rows = conn
        .query(
            "SELECT * FROM users WHERE email = ?",
            &[DatabaseValue::Text(email.to_string())],
        )
        .map_err(|err| err.to_string())?;
    let mut users = rows
        .iter()
        .map(auth_user_from_row)
        .collect::<Vec<_>>();
    users.sort_by_key(|user| user.numeric_id.unwrap_or_default());
    Ok(users)
}

fn local_cookie_request(req: &Request) -> bool {
    req.headers
        .get("host")
        .map(|host| {
            let host = host.to_ascii_lowercase();
            host.starts_with("127.0.0.1")
                || host.starts_with("localhost")
                || host.starts_with("[::1]")
        })
        .unwrap_or(false)
}

fn build_cookie_header(req: &Request, token: &str, max_age: u64) -> String {
    let secure = if local_cookie_request(req) {
        ""
    } else {
        "; Secure"
    };
    format!(
        "{SESSION_COOKIE_NAME}={token}; Path=/; Max-Age={max_age}; SameSite=Lax; HttpOnly{secure}"
    )
}

fn build_logout_cookie_header(req: &Request) -> String {
    let secure = if local_cookie_request(req) {
        ""
    } else {
        "; Secure"
    };
    format!(
        "{SESSION_COOKIE_NAME}=; Path=/; Max-Age=0; SameSite=Lax; HttpOnly{secure}"
    )
}

fn escape_html_attribute(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '"' => escaped.push_str("&quot;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{:02X}", byte)),
        }
    }
    encoded
}

fn create_session(
    conn: &mut AgiDbConnection,
    user_id: &str,
    csrf_secret: Option<String>,
) -> Result<(String, AuthSessionRecord), String> {
    let token = SessionToken::generate(Duration::from_secs(SESSION_TTL_SECS))
        .map_err(|err| err.to_string())?
        .token;
    let token_hash = hash_session_token(&token);
    let csrf_secret = csrf_secret.unwrap_or_else(|| hash_session_token(&(token.clone() + ":csrf")));
    let now = now_unix() as i64;
    let expires_at = now + SESSION_TTL_SECS as i64;
    conn.execute(
        "INSERT INTO sessions (user_id, token, expires_at, created_at, csrf_secret) VALUES (?, ?, ?, ?, ?)",
        &[
            match user_id.parse::<i64>() {
                Ok(value) => DatabaseValue::Integer(value),
                Err(_) => DatabaseValue::Text(user_id.to_string()),
            },
            DatabaseValue::Text(token_hash.clone()),
            DatabaseValue::Integer(expires_at),
            DatabaseValue::Integer(now),
            DatabaseValue::Text(csrf_secret.clone()),
        ],
    )
    .map_err(|err| err.to_string())?;
    if !user_id.is_empty() {
        enforce_max_active_sessions(conn, user_id)?;
    }
    Ok((
        token,
        AuthSessionRecord {
            token_hash,
            user_id: user_id.to_string(),
            csrf_secret,
            expires_at: expires_at as u64,
        },
    ))
}

fn enforce_max_active_sessions(conn: &mut AgiDbConnection, user_id: &str) -> Result<(), String> {
    let mut rows = conn
        .query(
            "SELECT * FROM sessions WHERE user_id = ?",
            &[DatabaseValue::Text(user_id.to_string())],
        )
        .map_err(|err| err.to_string())?;
    rows.sort_by_key(|row| row_integer(row, "created_at").unwrap_or_default());
    while rows.len() > MAX_ACTIVE_USER_SESSIONS {
        let row = rows.remove(0);
        if let Some(token_hash) = row_text(&row, "token") {
            conn.execute(
                "DELETE FROM sessions WHERE token = ?",
                &[DatabaseValue::Text(token_hash.to_string())],
            )
            .ok();
        }
    }
    Ok(())
}

fn delete_session(conn: &mut AgiDbConnection, token_hash: &str) {
    let _ = conn.execute(
        "DELETE FROM sessions WHERE token = ?",
        &[DatabaseValue::Text(token_hash.to_string())],
    );
    unregister_webrtc_session_peers(token_hash);
}

fn consume_rate_limit(bucket: &str, limit: usize) -> bool {
    let limiter = AUTH_RATE_LIMITER.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = limiter.lock().unwrap();
    let now = now_unix();
    let entries = guard.entry(bucket.to_string()).or_default();
    entries.retain(|timestamp| now.saturating_sub(*timestamp) < RATE_LIMIT_WINDOW_SECS);
    if entries.len() >= limit {
        return false;
    }
    entries.push(now);
    true
}

#[cfg(test)]
fn clear_auth_runtime_state() {
    if let Some(rate_limiter) = AUTH_RATE_LIMITER.get() {
        rate_limiter.lock().unwrap().clear();
    }
    if let Some(registrations) = ACTIVE_REGISTRATIONS.get() {
        registrations.lock().unwrap().clear();
    }
    if let Some(peers) = WEBRTC_PEER_REGISTRY.get() {
        peers.lock().unwrap().clear();
    }
    if let Some(meetings) = WEBRTC_MEETING_REGISTRY.get() {
        meetings.lock().unwrap().clear();
    }
}

fn hidden_csrf_field(secret: &str) -> String {
    format!(r#"<input type="hidden" name="_csrf" value="{secret}">"#)
}

fn webrtc_signaling_hub() -> &'static SignalingHub {
    WEBRTC_SIGNALING_HUB.get_or_init(SignalingHub::default)
}

fn webrtc_peer_registry() -> &'static Mutex<HashMap<String, WebRtcPeerRegistration>> {
    WEBRTC_PEER_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn webrtc_meeting_registry() -> &'static Mutex<HashMap<String, WebRtcMeetingRegistration>> {
    WEBRTC_MEETING_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn register_webrtc_peer(
    peer_id: &str,
    meeting_id: Option<&str>,
    role: Option<&str>,
    session: &AuthSessionRecord,
    user: &AuthUserRecord,
) -> Result<(), Response> {
    let meeting_id = meeting_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let role = role
        .map(str::trim)
        .filter(|value| *value == "host" || *value == "participant")
        .unwrap_or("participant")
        .to_string();
    let registration = WebRtcPeerRegistration {
        session_token_hash: session.token_hash.clone(),
        user_id: user.id.clone(),
        email: user.email.clone(),
        meeting_id: meeting_id.clone(),
    };

    let mut registry = webrtc_peer_registry().lock().unwrap();
    match registry.get(peer_id) {
        Some(existing)
            if existing.session_token_hash != session.token_hash || existing.user_id != user.id =>
        {
            Err(build_json_response(
                409,
                serde_json::json!({ "error": "peer_id is already registered" }).to_string(),
            ))
        }
        _ => {
            registry.insert(peer_id.to_string(), registration);
            drop(registry);
            if let Some(meeting_id) = meeting_id {
                let mut meetings = webrtc_meeting_registry().lock().unwrap();
                let meeting = meetings.entry(meeting_id).or_default();
                meeting.members.insert(
                    peer_id.to_string(),
                    WebRtcMeetingMember {
                        peer_id: peer_id.to_string(),
                        user_id: user.id.clone(),
                        email: user.email.clone(),
                        role: role.clone(),
                    },
                );
                if role == "host" || meeting.host_peer_id.is_none() {
                    meeting.host_peer_id = Some(peer_id.to_string());
                }
            }
            Ok(())
        }
    }
}

fn registered_webrtc_peer(peer_id: &str) -> Option<WebRtcPeerRegistration> {
    webrtc_peer_registry().lock().unwrap().get(peer_id).cloned()
}

fn unregister_webrtc_session_peers(token_hash: &str) {
    let mut registry = webrtc_peer_registry().lock().unwrap();
    let removed = registry
        .iter()
        .filter(|(_, registration)| registration.session_token_hash == token_hash)
        .map(|(peer_id, registration)| (peer_id.clone(), registration.meeting_id.clone()))
        .collect::<Vec<_>>();
    registry.retain(|_, registration| registration.session_token_hash != token_hash);
    drop(registry);

    let mut meetings = webrtc_meeting_registry().lock().unwrap();
    for (peer_id, meeting_id) in removed {
        let Some(meeting_id) = meeting_id else {
            continue;
        };
        let Some(meeting) = meetings.get_mut(&meeting_id) else {
            continue;
        };
        meeting.members.remove(&peer_id);
        if meeting.host_peer_id.as_deref() == Some(peer_id.as_str()) {
            meeting.host_peer_id = meeting
                .members
                .values()
                .find(|member| member.role == "host")
                .map(|member| member.peer_id.clone())
                .or_else(|| meeting.members.keys().next().cloned());
        }
        if meeting.members.is_empty() {
            meetings.remove(&meeting_id);
        }
    }
}

fn meeting_status_payload(meeting_id: &str) -> serde_json::Value {
    let meetings = webrtc_meeting_registry().lock().unwrap();
    if let Some(meeting) = meetings.get(meeting_id) {
        serde_json::json!({
            "meeting_id": meeting_id,
            "host_peer_id": meeting.host_peer_id,
            "member_count": meeting.members.len(),
            "members": meeting.members.values().map(|member| serde_json::json!({
                "peer_id": member.peer_id,
                "user_id": member.user_id,
                "email": member.email,
                "role": member.role,
            })).collect::<Vec<_>>(),
        })
    } else {
        serde_json::json!({
            "meeting_id": meeting_id,
            "host_peer_id": serde_json::Value::Null,
            "member_count": 0,
            "members": [],
        })
    }
}

fn log_webrtc_trace(event: &str, details: serde_json::Value) {
    eprintln!("[queral-webrtc] {event} {details}");
}

fn block_on_runtime<F, T>(future: F) -> Result<T, String>
where
    F: std::future::Future<Output = T>,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        Ok(handle.block_on(future))
    } else {
        let runtime = tokio::runtime::Runtime::new().map_err(|error| error.to_string())?;
        Ok(runtime.block_on(future))
    }
}

fn write_framework_response(stream: &mut TcpStream, response: Response) -> std::io::Result<()> {
    let status_line = match response.status {
        200 => "HTTP/1.1 200 OK",
        202 => "HTTP/1.1 202 Accepted",
        201 => "HTTP/1.1 201 Created",
        302 => "HTTP/1.1 302 Found",
        400 => "HTTP/1.1 400 Bad Request",
        401 => "HTTP/1.1 401 Unauthorized",
        403 => "HTTP/1.1 403 Forbidden",
        404 => "HTTP/1.1 404 Not Found",
        405 => "HTTP/1.1 405 Method Not Allowed",
        409 => "HTTP/1.1 409 Conflict",
        422 => "HTTP/1.1 422 Unprocessable Entity",
        429 => "HTTP/1.1 429 Too Many Requests",
        _ => "HTTP/1.1 500 Internal Server Error",
    };
    let mut header_block = String::new();
    let mut has_content_type = false;
    for (key, value) in &response.headers {
        if key.eq_ignore_ascii_case("content-type") {
            has_content_type = true;
        }
        header_block.push_str(&format!("{key}: {value}\r\n"));
    }
    if !has_content_type {
        header_block.push_str("Content-Type: text/html; charset=utf-8\r\n");
    }
    let head = format!(
        "{status_line}\r\nContent-Length: {}\r\nConnection: close\r\n{header_block}\r\n",
        response.body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(&response.body)?;
    stream.flush()
}

fn render_auth_view(
    view_engine: &ViewEngine,
    view: &str,
    title: &str,
    csrf_secret: &str,
    error_message: Option<&str>,
    return_to: &str,
) -> Result<Response, String> {
    let mut data = HashMap::new();
    data.insert("title".to_string(), title.to_string());
    data.insert("csrf_field".to_string(), hidden_csrf_field(csrf_secret));
    data.insert("csrf_token".to_string(), csrf_secret.to_string());
    data.insert("return_to".to_string(), return_to.to_string());
    data.insert(
        "return_to_query".to_string(),
        if return_to.is_empty() {
            String::new()
        } else {
            format!("?return_to={}", percent_encode(return_to))
        },
    );
    data.insert(
        "return_to_field".to_string(),
        if return_to.is_empty() {
            String::new()
        } else {
            format!(
                r#"<input type="hidden" name="return_to" value="{}">"#,
                escape_html_attribute(return_to)
            )
        },
    );
    data.insert(
        "error_message".to_string(),
        error_message
            .map(|message| format!(r#"<p class="form-message" data-type="error">{message}</p>"#))
            .unwrap_or_default(),
    );
    let html = view_engine.render(view, &data)?;
    Ok(Response::html(&html))
}

fn redirect_response(location: &str) -> Response {
    let mut headers = HashMap::new();
    headers.insert("Location".to_string(), location.to_string());
    Response {
        status: 302,
        headers,
        body: Vec::new(),
    }
}

fn normalized_return_to(req: &Request) -> String {
    normalize_return_to_value(req.query("return_to").unwrap_or(""))
}

fn normalize_return_to_value(raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() {
        return String::new();
    }
    if !raw.starts_with('/') || raw.starts_with("//") {
        return String::new();
    }
    raw.to_string()
}

fn dashboard_or_return_to(return_to: &str) -> &str {
    if return_to.is_empty() {
        "/dashboard"
    } else {
        return_to
    }
}

fn login_location(return_to: &str) -> String {
    if return_to.is_empty() {
        "/login".to_string()
    } else {
        format!("/login?return_to={}", percent_encode(return_to))
    }
}

fn build_json_response(status: u16, body: String) -> Response {
    let mut response = Response::json(&body);
    response.status = status;
    response
}

fn build_text_response(status: u16, content_type: &str, body: String) -> Response {
    let mut headers = HashMap::new();
    headers.insert("Content-Type".to_string(), content_type.to_string());
    Response {
        status,
        headers,
        body: body.into_bytes(),
    }
}

fn split_top_level(source: &str, separator: char) -> Vec<String> {
    let mut parts = vec![];
    let mut current = String::new();
    let mut paren_depth = 0;
    let mut brace_depth = 0;
    let mut bracket_depth = 0;
    let mut in_double_quote = false;
    let mut in_single_quote = false;
    let mut escaped = false;

    for ch in source.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }

        match ch {
            '\\' => {
                current.push(ch);
                escaped = true;
            }
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
                current.push(ch);
            }
            '\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
                current.push(ch);
            }
            _ => {
                if !in_double_quote && !in_single_quote {
                    match ch {
                        '(' => paren_depth += 1,
                        ')' => paren_depth -= 1,
                        '{' => brace_depth += 1,
                        '}' => brace_depth -= 1,
                        '[' => bracket_depth += 1,
                        ']' => bracket_depth -= 1,
                        _ => {}
                    }
                    if ch == separator && paren_depth == 0 && brace_depth == 0 && bracket_depth == 0
                    {
                        parts.push(current.trim().to_string());
                        current.clear();
                        continue;
                    }
                }
                current.push(ch);
            }
        }
    }

    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }

    parts
}

fn parse_json_value(expr: &str) -> JsonValue {
    let expr = expr.trim();
    if expr.is_empty() {
        return JsonValue::Null;
    }
    if (expr.starts_with('"') && expr.ends_with('"'))
        || (expr.starts_with('\'') && expr.ends_with('\''))
    {
        return JsonValue::String(expr[1..expr.len() - 1].to_string());
    }
    if expr == "true" {
        return JsonValue::Bool(true);
    }
    if expr == "false" {
        return JsonValue::Bool(false);
    }
    if expr == "null" {
        return JsonValue::Null;
    }
    if expr.starts_with('[') && expr.ends_with(']') {
        let inner = &expr[1..expr.len() - 1];
        let values = split_top_level(inner, ',')
            .into_iter()
            .map(|part| parse_json_value(&part))
            .collect();
        return JsonValue::Array(values);
    }
    if expr.starts_with('{') && expr.ends_with('}') {
        let inner = &expr[1..expr.len() - 1];
        let mut entries = vec![];
        for pair in split_top_level(inner, ',') {
            if let Some(colon_pos) = pair.find(':') {
                let key = pair[..colon_pos]
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string();
                let value = parse_json_value(&pair[colon_pos + 1..]);
                entries.push((key, value));
            }
        }
        return JsonValue::Object(entries);
    }
    if expr
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '.' | '-'))
    {
        return JsonValue::Number(expr.to_string());
    }
    JsonValue::Unsupported(expr.to_string())
}

fn handle_auth_request(
    req: &Request,
    view_engine: &ViewEngine,
    project_root: &Path,
) -> Result<Option<Response>, String> {
    let path = req.path();
    let return_to = normalized_return_to(req);
    let is_auth_page = matches!(path, "/login" | "/register");
    let is_logout = path == "/logout" || path == "/api/auth/logout";
    let is_current_user = path == "/api/auth/me";
    let is_protected =
        path == "/dashboard" || path == "/dashboard/user" || path == "/dashboard/admin";

    if !is_auth_page && !is_logout && !is_current_user && !is_protected {
        return Ok(None);
    }

    let mut conn = open_auth_db(project_root)?;
    let existing_session = load_session_by_cookie(&mut conn, req)?;
    let existing_user = if let Some(session) = &existing_session {
        load_user_by_id(&mut conn, &session.user_id)?
    } else {
        None
    };

    if req.method == HttpMethod::Get && is_auth_page {
        if existing_user.is_some() {
            return Ok(Some(redirect_response(dashboard_or_return_to(&return_to))));
        }
        let (cookie_token, session) = match existing_session {
            Some(session) => {
                let raw = parse_cookies(req)
                    .get(SESSION_COOKIE_NAME)
                    .cloned()
                    .unwrap_or_default();
                (raw, session)
            }
            None => create_session(&mut conn, "", None)?,
        };
        let mut response = if path == "/login" {
            render_auth_view(
                view_engine,
                "auth/login",
                "Sign in",
                &session.csrf_secret,
                None,
                &return_to,
            )?
        } else {
            render_auth_view(
                view_engine,
                "auth/register",
                "Create account",
                &session.csrf_secret,
                None,
                &return_to,
            )?
        };
        response.headers.insert(
            "Set-Cookie".to_string(),
            build_cookie_header(req, &cookie_token, SESSION_TTL_SECS),
        );
        return Ok(Some(response));
    }

    if is_current_user {
        if let Some(user) = existing_user {
            let json = serde_json::json!({
                "authenticated": true,
                "user": {
                    "id": user.id,
                    "name": user.name,
                    "email": user.email,
                    "role": user.role,
                }
            });
            return Ok(Some(Response::json(&json.to_string())));
        }
        let mut response = Response::json(r#"{"authenticated":false}"#);
        response.status = 401;
        return Ok(Some(response));
    }

    if is_protected {
        let Some(user) = existing_user.clone() else {
            let location = login_location(&req.path);
            return Ok(Some(redirect_response(&location)));
        };
        if path == "/dashboard/admin" && user.role != "admin" {
            let mut response =
                Response::html("<h1>403 Forbidden</h1><p>Administrator access required.</p>");
            response.status = 403;
            return Ok(Some(response));
        }
        let csrf_secret = existing_session
            .as_ref()
            .map(|session| session.csrf_secret.clone())
            .unwrap_or_default();
        let mut data = HashMap::new();
        data.insert(
            "title".to_string(),
            if path == "/dashboard/admin" {
                "Admin Dashboard".to_string()
            } else {
                "Dashboard".to_string()
            },
        );
        data.insert("database".to_string(), resolved_auth_db_path(project_root));
        data.insert("user_name".to_string(), user.name.clone());
        data.insert("error_message".to_string(), String::new());
        data.insert("csrf_token".to_string(), csrf_secret);
        let view = if path == "/dashboard/admin" {
            "dashboard/admin"
        } else {
            "dashboard/user"
        };
        let html = view_engine.render(view, &data)?;
        return Ok(Some(Response::html(&html)));
    }

    if req.method != HttpMethod::Post || (!is_auth_page && !is_logout) {
        return Ok(None);
    }

    let bucket_ip = req
        .remote_addr
        .map(|addr| addr.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let form = parse_form_body(req);
    let form_return_to = form
        .get("return_to")
        .and_then(|values| values.first())
        .map(|value| normalize_return_to_value(value))
        .unwrap_or_default();
    let return_to = if form_return_to.is_empty() {
        return_to
    } else {
        form_return_to
    };
    let csrf = form
        .get("_csrf")
        .and_then(|values| values.first())
        .cloned()
        .unwrap_or_default();
        let Some(session) = existing_session else {
        if is_logout {
            let mut response = redirect_response("/login");
            response
                .headers
                .insert("Set-Cookie".to_string(), build_logout_cookie_header(req));
            return Ok(Some(response));
        }
        let mut response =
            Response::html("<h1>400 Bad Request</h1><p>Missing session context.</p>");
        response.status = 400;
        return Ok(Some(response));
    };
    if session.csrf_secret != csrf {
        let mut response = Response::html("<h1>403 Forbidden</h1><p>Invalid CSRF token.</p>");
        response.status = 403;
        return Ok(Some(response));
    }

    if is_logout {
        delete_session(&mut conn, &session.token_hash);
        let mut response = redirect_response("/login");
        response
            .headers
            .insert("Set-Cookie".to_string(), build_logout_cookie_header(req));
        return Ok(Some(response));
    }

    if path == "/register" {
        if !consume_rate_limit(&format!("register:{bucket_ip}"), RATE_LIMIT_REGISTER_MAX) {
            let mut response =
                Response::html("<h1>429 Too Many Requests</h1><p>Please try again later.</p>");
            response.status = 429;
            return Ok(Some(response));
        }
        let name = form
            .get("name")
            .and_then(|values| values.first())
            .cloned()
            .unwrap_or_default();
        let email = form
            .get("email")
            .and_then(|values| values.first())
            .map(|value| value.trim().to_lowercase())
            .unwrap_or_default();
        let password = form
            .get("password")
            .and_then(|values| values.first())
            .cloned()
            .unwrap_or_default();
        if name.is_empty() || email.is_empty() || password.len() < 8 {
            let mut response = render_auth_view(
                view_engine,
                "auth/register",
                "Create account",
                &session.csrf_secret,
                Some("Please complete all fields and use a longer password."),
                &return_to,
            )?;
            response.status = 400;
            return Ok(Some(response));
        }
        let _registration_guard = RegistrationGuard::acquire(&email);
        let mut conn = open_auth_db(project_root)?;
        let duplicate = load_users_by_email(&mut conn, &email)?;
        if !duplicate.is_empty() {
            let mut response = render_auth_view(
                view_engine,
                "auth/register",
                "Create account",
                &session.csrf_secret,
                Some("The email address is already registered."),
                &return_to,
            )?;
            response.status = 409;
            return Ok(Some(response));
        }
        let hasher = Argon2idPasswordHasher::default();
        let hash = hasher
            .hash(password.as_bytes())
            .map_err(|err| err.to_string())?;
        let now = now_unix() as i64;
        let inserted_email = email.clone();
        conn.execute(
            "INSERT INTO users (name, email, password_hash, role, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            &[
                DatabaseValue::Text(name.clone()),
                DatabaseValue::Text(email),
                DatabaseValue::Text(hash),
                DatabaseValue::Text("user".to_string()),
                DatabaseValue::Integer(now),
                DatabaseValue::Integer(now),
            ],
        )
        .map_err(|err| err.to_string())?;
        let inserted_users = load_users_by_email(&mut conn, &inserted_email)?;
        delete_session(&mut conn, &session.token_hash);
        let user_id = inserted_users
            .last()
            .map(|user| user.id.clone())
            .filter(|id| !id.is_empty())
            .ok_or_else(|| "AGIDB did not return the registered user id".to_string())?;
        let (cookie_token, _) = create_session(&mut conn, &user_id, None)?;
        let mut response = redirect_response(dashboard_or_return_to(&return_to));
        response.headers.insert(
            "Set-Cookie".to_string(),
            build_cookie_header(req, &cookie_token, SESSION_TTL_SECS),
        );
        return Ok(Some(response));
    }

    if path == "/login" {
        if !consume_rate_limit(&format!("login:{bucket_ip}"), RATE_LIMIT_LOGIN_MAX) {
            let mut response = render_auth_view(
                view_engine,
                "auth/login",
                "Sign in",
                &session.csrf_secret,
                Some("Too many attempts. Please try again later."),
                &return_to,
            )?;
            response.status = 429;
            return Ok(Some(response));
        }
        let email = form
            .get("email")
            .and_then(|values| values.first())
            .map(|value| value.trim().to_lowercase())
            .unwrap_or_default();
        let password = form
            .get("password")
            .and_then(|values| values.first())
            .cloned()
            .unwrap_or_default();
        let generic_error = "Invalid credentials.";
        let users = load_users_by_email(&mut conn, &email)?;
        let Some(user) = users.iter().rev().find_map(|candidate| {
            let hasher = Argon2idPasswordHasher::default();
            match hasher.verify(password.as_bytes(), &candidate.password_hash) {
                Ok(true) => Some(candidate.clone()),
                _ => None,
            }
        }) else {
            let mut response = render_auth_view(
                view_engine,
                "auth/login",
                "Sign in",
                &session.csrf_secret,
                Some(generic_error),
                &return_to,
            )?;
            response.status = 401;
            return Ok(Some(response));
        };
        delete_session(&mut conn, &session.token_hash);
        let (cookie_token, _) = create_session(&mut conn, &user.id, None)?;
        let mut response = redirect_response(dashboard_or_return_to(&return_to));
        response.headers.insert(
            "Set-Cookie".to_string(),
            build_cookie_header(req, &cookie_token, SESSION_TTL_SECS),
        );
        return Ok(Some(response));
    }

    Ok(None)
}

fn builtin_api_response(path: &str) -> Option<String> {
    match path {
        "/api/framework/status" => Some(framework_status_json()),
        "/api/runtime/status" => Some(
            r#"{"platform":"native","compute_mode":"CPU","total_memory":"available","storage":"AGIDB","cpu_usage":"12%","memory_usage":"6.4 GB"}"#
                .to_string(),
        ),
        "/api/blockchain/status" => {
            Some(r#"{"height":1923001,"network":"SIBAQ","status":"synced"}"#.to_string())
        }
        "/api/auth/status" => {
            Some(r#"{"enabled":true,"roles":["user","admin"],"sessions":"agidb-server"}"#.to_string())
        }
        "/api/crud/status" => {
            Some(r#"{"enabled":true,"storage":"agidb-server","entities":["users","projects","tasks"]}"#.to_string())
        }
        _ => None,
    }
}

fn framework_status_json() -> String {
    let webrtc = agilang_runtime_webrtc::capabilities();
    serde_json::json!({
        "http_address": "127.0.0.1:8080",
        "database_status": "AGIDB ready",
        "websocket_status": "ready",
        "runtime_status": "native",
        "webrtc_status": if webrtc.peer_connection { "native" } else { "signaling-only" },
        "webrtc_signaling": webrtc.signaling,
        "webrtc_peer_connection": webrtc.peer_connection,
        "webrtc_data_channel": webrtc.data_channel,
        "stun_turn_status": if webrtc.turn_server { "provider-ready" } else { "not-enabled" },
        "webrtc_explanation": webrtc.explanation,
        "ai_status": "native tensors",
        "abi_version": "1.1",
        "connected_users": 1
    })
    .to_string()
}

pub(crate) fn handle_webrtc_request(
    req: &Request,
    project_root: &Path,
) -> Result<Option<Response>, String> {
    let path = req.path();
    let is_webrtc_path = matches!(
        path,
        "/api/webrtc/status"
            | "/api/webrtc/register"
            | "/api/webrtc/meeting"
            | "/api/webrtc/signal"
            | "/api/webrtc/poll"
    );
    if !is_webrtc_path {
        return Ok(None);
    }

    let capabilities = agilang_runtime_webrtc::capabilities();
    if path == "/api/webrtc/status" {
        let active_peer_count = webrtc_peer_registry().lock().unwrap().len();
        let status = serde_json::json!({
            "signaling": capabilities.signaling,
            "peer_connection": capabilities.peer_connection,
            "data_channel": capabilities.data_channel,
            "stun_server": capabilities.stun_server,
            "turn_server": capabilities.turn_server,
            "active_peers": active_peer_count,
            "explanation": capabilities.explanation,
        });
        return Ok(Some(build_json_response(200, status.to_string())));
    }

    let mut conn = open_auth_db(project_root)?;
    let Some(session) = load_session_by_cookie(&mut conn, req)? else {
        let mut response = Response::json(r#"{"error":"Unauthorized"}"#);
        response.status = 401;
        return Ok(Some(response));
    };
    let Some(user) = load_user_by_id(&mut conn, &session.user_id)? else {
        let mut response = Response::json(r#"{"error":"Unauthorized"}"#);
        response.status = 401;
        return Ok(Some(response));
    };

    match path {
        "/api/webrtc/register" => {
            if req.method != HttpMethod::Post {
                return Ok(Some(build_text_response(
                    405,
                    "application/json",
                    r#"{"error":"Method Not Allowed"}"#.to_string(),
                )));
            }
            if let Some(response) = verify_webrtc_csrf(req, &session) {
                return Ok(Some(response));
            }
            let payload = match parse_json_body(req) {
                Ok(payload) => payload,
                Err(error) => {
                    return Ok(Some(build_json_response(
                        422,
                        serde_json::json!({ "error": error }).to_string(),
                    )))
                }
            };
            let peer_id = payload
                .get("peer_id")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            let meeting_id = payload
                .get("meeting_id")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            let role = payload
                .get("role")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            if peer_id.is_empty() {
                return Ok(Some(build_json_response(
                    422,
                    serde_json::json!({ "error": "peer_id is required" }).to_string(),
                )));
            }
            if let Err(response) =
                register_webrtc_peer(&peer_id, meeting_id.as_deref(), role.as_deref(), &session, &user)
            {
                return Ok(Some(response));
            }
            let meeting = meeting_id
                .as_deref()
                .map(meeting_status_payload)
                .unwrap_or_else(|| serde_json::json!(null));
            let body = serde_json::json!({
                "registered": true,
                "peer_id": peer_id,
                "user": {
                    "id": user.id,
                    "email": user.email,
                    "role": user.role,
                },
                "active_peers": webrtc_peer_registry().lock().unwrap().len(),
                "meeting": meeting,
            });
            log_webrtc_trace(
                "register",
                serde_json::json!({
                    "meeting_id": meeting_id,
                    "peer_id": peer_id,
                    "user_id": user.id,
                    "role": role,
                    "active_peers": webrtc_peer_registry().lock().unwrap().len(),
                }),
            );
            Ok(Some(build_json_response(200, body.to_string())))
        }
        "/api/webrtc/meeting" => {
            if req.method != HttpMethod::Get {
                return Ok(Some(build_text_response(
                    405,
                    "application/json",
                    r#"{"error":"Method Not Allowed"}"#.to_string(),
                )));
            }
            let meeting_id = req.query("meeting_id").unwrap_or("").trim().to_string();
            if meeting_id.is_empty() {
                return Ok(Some(build_json_response(
                    422,
                    serde_json::json!({ "error": "meeting_id is required" }).to_string(),
                )));
            }
            log_webrtc_trace(
                "meeting_lookup",
                serde_json::json!({
                    "meeting_id": meeting_id,
                    "user_id": user.id,
                }),
            );
            Ok(Some(build_json_response(
                200,
                meeting_status_payload(&meeting_id).to_string(),
            )))
        }
        "/api/webrtc/signal" => {
            if req.method != HttpMethod::Post {
                return Ok(Some(build_text_response(
                    405,
                    "application/json",
                    r#"{"error":"Method Not Allowed"}"#.to_string(),
                )));
            }
            if let Some(response) = verify_webrtc_csrf(req, &session) {
                return Ok(Some(response));
            }
            let payload = match parse_json_body(req) {
                Ok(payload) => payload,
                Err(error) => {
                    return Ok(Some(build_json_response(
                        422,
                        serde_json::json!({ "error": error }).to_string(),
                    )))
                }
            };
            let from = payload
                .get("from")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            let to = payload
                .get("to")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            let kind = payload
                .get("kind")
                .and_then(|value| value.as_str())
                .and_then(parse_signal_kind);
            let signal_payload = payload
                .get("payload")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string();

            if from.is_empty() || to.is_empty() || signal_payload.is_empty() || kind.is_none() {
                return Ok(Some(build_json_response(
                    422,
                    serde_json::json!({ "error": "from, to, kind and payload are required" })
                        .to_string(),
                )));
            }

            let kind = kind.expect("validated above");
            let signal_kind = format_signal_kind(&kind).to_string();
            let signal_payload_size = signal_payload.len();
            let log_from = from.clone();
            let log_to = to.clone();
            let Some(from_registration) = registered_webrtc_peer(&from) else {
                return Ok(Some(build_json_response(
                    409,
                    serde_json::json!({ "error": "source peer is not registered" }).to_string(),
                )));
            };
            if from_registration.session_token_hash != session.token_hash
                || from_registration.user_id != user.id
            {
                return Ok(Some(build_json_response(
                    403,
                    serde_json::json!({ "error": "peer ownership mismatch" }).to_string(),
                )));
            }
            let Some(destination_registration) = registered_webrtc_peer(&to) else {
                return Ok(Some(build_json_response(
                    404,
                    serde_json::json!({ "error": "destination peer is not registered" })
                        .to_string(),
                )));
            };
            block_on_runtime(async {
                webrtc_signaling_hub()
                    .publish(SignalEnvelope {
                        session_id: uuid::Uuid::new_v4(),
                        from,
                        to,
                        kind,
                        payload: signal_payload,
                    })
                    .await;
            })?;
            log_webrtc_trace(
                "signal_publish",
                serde_json::json!({
                    "meeting_id": from_registration.meeting_id,
                    "kind": signal_kind,
                    "from": log_from,
                    "to": log_to,
                    "from_user_id": from_registration.user_id,
                    "to_user_id": destination_registration.user_id,
                    "payload_size": signal_payload_size,
                }),
            );
            Ok(Some(build_json_response(
                202,
                serde_json::json!({
                    "queued": true,
                    "to_user": {
                        "id": destination_registration.user_id,
                        "email": destination_registration.email,
                    }
                })
                .to_string(),
            )))
        }
        "/api/webrtc/poll" => {
            if req.method != HttpMethod::Get {
                return Ok(Some(build_text_response(
                    405,
                    "application/json",
                    r#"{"error":"Method Not Allowed"}"#.to_string(),
                )));
            }
            let peer_id = req.query("peer_id").unwrap_or("").trim().to_string();
            if peer_id.is_empty() {
                return Ok(Some(build_json_response(
                    422,
                    serde_json::json!({ "error": "peer_id is required" }).to_string(),
                )));
            }
            let Some(registration) = registered_webrtc_peer(&peer_id) else {
                return Ok(Some(build_json_response(
                    404,
                    serde_json::json!({ "error": "peer_id is not registered" }).to_string(),
                )));
            };
            if registration.session_token_hash != session.token_hash || registration.user_id != user.id {
                return Ok(Some(build_json_response(
                    403,
                    serde_json::json!({ "error": "peer ownership mismatch" }).to_string(),
                )));
            }
            let message = block_on_runtime(async { webrtc_signaling_hub().receive(&peer_id).await })?;
            if let Some(ref message) = message {
                log_webrtc_trace(
                    "signal_deliver",
                    serde_json::json!({
                        "meeting_id": registration.meeting_id,
                        "peer_id": peer_id,
                        "kind": format_signal_kind(&message.kind),
                        "from": message.from,
                        "to": message.to,
                        "payload_size": message.payload.len(),
                    }),
                );
            }
            let body = if let Some(message) = message {
                serde_json::json!({
                    "message": {
                        "session_id": message.session_id,
                        "from": message.from,
                        "to": message.to,
                        "kind": format_signal_kind(&message.kind),
                        "payload": message.payload,
                    }
                })
            } else {
                serde_json::json!({ "message": null })
            };
            Ok(Some(build_json_response(200, body.to_string())))
        }
        _ => Ok(None),
    }
}

fn verify_webrtc_csrf(req: &Request, session: &AuthSessionRecord) -> Option<Response> {
    let token = req
        .headers
        .get("x-csrf-token")
        .or_else(|| req.headers.get("X-CSRF-Token"))
        .cloned()
        .unwrap_or_default();
    if token == session.csrf_secret {
        None
    } else {
        Some(build_json_response(
            403,
            serde_json::json!({ "error": "Invalid CSRF token." }).to_string(),
        ))
    }
}

fn parse_json_body(req: &Request) -> Result<serde_json::Value, String> {
    serde_json::from_slice(&req.body).map_err(|error| error.to_string())
}

fn parse_signal_kind(value: &str) -> Option<SignalKind> {
    match value {
        "offer" => Some(SignalKind::Offer),
        "answer" => Some(SignalKind::Answer),
        "ice-candidate" => Some(SignalKind::IceCandidate),
        "hangup" => Some(SignalKind::Hangup),
        _ => None,
    }
}

fn format_signal_kind(kind: &SignalKind) -> &'static str {
    match kind {
        SignalKind::Offer => "offer",
        SignalKind::Answer => "answer",
        SignalKind::IceCandidate => "ice-candidate",
        SignalKind::Hangup => "hangup",
    }
}

fn cj_http_client() -> Result<agilang_runtime_http::HttpClient, String> {
    agilang_runtime_http::HttpClient::new(agilang_runtime_http::TlsPolicy {
        require_https: false,
        allow_invalid_certificates: false,
    })
    .map_err(|err| err.to_string())
}

fn cj_api_base(project_root: &Path) -> String {
    agilang_framework_config::Config::load(project_root)
        .get(
            "CJ_API_BASE",
            "https://developers.cjdropshipping.com/api2.0/v1",
        )
        .trim_end_matches('/')
        .to_string()
}

fn cj_token(project_root: &Path) -> Result<Option<String>, String> {
    let config = agilang_framework_config::Config::load(project_root);
    let existing = config.get("CJ_ACCESS_TOKEN", "");
    if !existing.is_empty() {
        return Ok(Some(existing));
    }
    let api_key = config.get("CJ_API_KEY", "");
    if api_key.is_empty() {
        return Ok(None);
    }

    let client = cj_http_client()?;
    let url = format!(
        "{}/authentication/getAccessToken",
        cj_api_base(project_root)
    );
    let body = serde_json::json!({ "apiKey": api_key })
        .to_string()
        .into_bytes();
    let request = agilang_runtime_http::HttpRequest {
        method: "POST".to_string(),
        url,
        query: BTreeMap::new(),
        headers: BTreeMap::from([("Content-Type".to_string(), "application/json".to_string())]),
        body,
        timeout_ms: Some(30_000),
    };
    let response = agilang_runtime_async::block_on(client.execute(request))
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())?;
    let payload: serde_json::Value =
        serde_json::from_slice(&response.body).map_err(|err| err.to_string())?;
    Ok(payload
        .get("data")
        .and_then(|data| data.get("accessToken"))
        .and_then(|value| value.as_str())
        .map(str::to_string))
}

fn cj_request(
    project_root: &Path,
    method: &str,
    endpoint: &str,
    query: BTreeMap<String, Vec<String>>,
    body: Option<serde_json::Value>,
) -> Result<agilang_runtime_http::HttpResponse, String> {
    let mut headers = BTreeMap::from([("Accept".to_string(), "application/json".to_string())]);
    if let Some(token) = cj_token(project_root)? {
        headers.insert("CJ-Access-Token".to_string(), token);
    }
    let body_bytes = if let Some(value) = body {
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        serde_json::to_vec(&value).map_err(|err| err.to_string())?
    } else {
        Vec::new()
    };
    let request = agilang_runtime_http::HttpRequest {
        method: method.to_string(),
        url: format!(
            "{}/{}",
            cj_api_base(project_root),
            endpoint.trim_start_matches('/')
        ),
        query,
        headers,
        body: body_bytes,
        timeout_ms: Some(30_000),
    };
    let client = cj_http_client()?;
    agilang_runtime_async::block_on(client.execute(request))
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())
}

fn normalize_cj_product(row: &serde_json::Value) -> serde_json::Value {
    let image = row
        .get("productImage")
        .or_else(|| row.get("bigImage"))
        .or_else(|| row.get("image"))
        .cloned()
        .unwrap_or(serde_json::Value::String(String::new()));
    let normalized_image = match image {
        serde_json::Value::Array(values) => values
            .into_iter()
            .next()
            .unwrap_or(serde_json::Value::String(String::new())),
        serde_json::Value::String(value) => {
            if value.trim_start().starts_with('[') {
                match serde_json::from_str::<serde_json::Value>(&value) {
                    Ok(serde_json::Value::Array(values)) => values
                        .into_iter()
                        .next()
                        .unwrap_or(serde_json::Value::String(String::new())),
                    _ => serde_json::Value::String(value),
                }
            } else {
                serde_json::Value::String(value)
            }
        }
        other => other,
    };
    serde_json::json!({
        "id": row.get("pid").or_else(|| row.get("id")).or_else(|| row.get("productId")).cloned().unwrap_or(serde_json::Value::Null),
        "sku": row.get("productSku").or_else(|| row.get("sku")).or_else(|| row.get("productCode")).cloned().unwrap_or(serde_json::Value::String(String::new())),
        "name": row.get("productNameEn").or_else(|| row.get("productName")).or_else(|| row.get("name")).cloned().unwrap_or(serde_json::Value::String(String::new())),
        "image": normalized_image,
        "description": row.get("description").or_else(|| row.get("productNameEn")).or_else(|| row.get("name")).cloned().unwrap_or(serde_json::Value::String(String::new())),
        "category": row.get("categoryName").or_else(|| row.get("category")).cloned().unwrap_or(serde_json::Value::String("CJdropshipping".to_string())),
        "price": row.get("sellPrice").or_else(|| row.get("price")).or_else(|| row.get("productPrice")).cloned().unwrap_or(serde_json::json!(0)),
        "stock": row.get("totalInventoryNum").or_else(|| row.get("stock")).cloned().unwrap_or(serde_json::json!(999))
    })
}

fn map_cj_order(order: &serde_json::Value, cj_items: &[serde_json::Value]) -> serde_json::Value {
    let customer = order
        .get("customer")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let products = cj_items
        .iter()
        .map(|item| {
            let mut product = serde_json::json!({
                "sku": item.get("cjSku").cloned().unwrap_or(serde_json::Value::String(String::new())),
                "quantity": item.get("quantity").cloned().unwrap_or(serde_json::json!(1))
            });
            if let Some(vid) = item.get("cjVid") {
                if !vid.is_null() && vid.as_str().unwrap_or_default() != "" {
                    product["vid"] = vid.clone();
                }
            }
            product
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "orderNumber": order.get("number").cloned().unwrap_or(serde_json::Value::String(String::new())),
        "fromCountryCode": customer.get("fromCountryCode").cloned().unwrap_or(serde_json::Value::Null),
        "logisticName": customer.get("logisticName").cloned().unwrap_or(serde_json::Value::Null),
        "shippingCountryCode": customer.get("country").cloned().unwrap_or(serde_json::Value::String("ZM".to_string())),
        "shippingCountry": customer.get("countryName").or_else(|| customer.get("country")).cloned().unwrap_or(serde_json::Value::String("Zambia".to_string())),
        "shippingProvince": customer.get("province").cloned().unwrap_or(serde_json::Value::String(String::new())),
        "shippingCity": customer.get("city").cloned().unwrap_or(serde_json::Value::String(String::new())),
        "shippingAddress": customer.get("address").cloned().unwrap_or(serde_json::Value::String(String::new())),
        "shippingCustomerName": customer.get("name").cloned().unwrap_or(serde_json::Value::String(String::new())),
        "shippingPhone": customer.get("phone").cloned().unwrap_or(serde_json::Value::String(String::new())),
        "remark": format!(
            "NOVA order {}; tx {}",
            order.get("number").and_then(|v| v.as_str()).unwrap_or_default(),
            order.get("txHash").and_then(|v| v.as_str()).unwrap_or("manual")
        ),
        "products": products
    })
}

fn native_cj_response(req: &Request, project_root: &Path) -> Option<(u16, String)> {
    let path = req.path();
    if !path.starts_with("/api/cj/") {
        return None;
    }

    let response = match (req.method.clone(), path) {
        (HttpMethod::Get, "/api/cj/health") => {
            let config = agilang_framework_config::Config::load(project_root);
            let configured = !config.get("CJ_API_KEY", "").is_empty()
                || !config.get("CJ_ACCESS_TOKEN", "").is_empty();
            Ok((
                200,
                serde_json::json!({
                    "ok": true,
                    "provider": "NOVA CJdropshipping",
                    "transport": "AGILANG native runtime HTTP",
                    "same_origin": true,
                    "cjConfigured": configured,
                    "nativeOutboundHttp": true,
                    "supports": ["http", "https"]
                })
                .to_string(),
            ))
        }
        (HttpMethod::Get, "/api/cj/products") => {
            let q = req.query("q").unwrap_or("").to_string();
            let page = req.query("page").unwrap_or("1").to_string();
            let query = BTreeMap::from([
                ("productNameEn".to_string(), vec![q]),
                ("pageNum".to_string(), vec![page]),
                ("pageSize".to_string(), vec!["20".to_string()]),
            ]);
            match cj_request(project_root, "GET", "/product/list", query, None) {
                Ok(resp) => {
                    let payload: serde_json::Value = serde_json::from_slice(&resp.body)
                        .unwrap_or_else(|_| serde_json::json!({}));
                    let rows = payload
                        .get("data")
                        .and_then(|data| data.get("list").or_else(|| data.get("content")))
                        .and_then(|rows| rows.as_array())
                        .cloned()
                        .unwrap_or_default();
                    let products = rows.iter().map(normalize_cj_product).collect::<Vec<_>>();
                    Ok((
                        resp.status,
                        serde_json::json!({ "products": products }).to_string(),
                    ))
                }
                Err(err) => Err(err),
            }
        }
        (HttpMethod::Get, "/api/cj/product") => {
            let id = req.query("id").unwrap_or("").to_string();
            let query = BTreeMap::from([("pid".to_string(), vec![id])]);
            match cj_request(project_root, "GET", "/product/query", query, None) {
                Ok(resp) => {
                    let payload: serde_json::Value = serde_json::from_slice(&resp.body)
                        .unwrap_or_else(|_| serde_json::json!({}));
                    let data = payload.get("data").unwrap_or(&payload);
                    Ok((
                        resp.status,
                        serde_json::json!({ "product": normalize_cj_product(data) }).to_string(),
                    ))
                }
                Err(err) => Err(err),
            }
        }
        (HttpMethod::Post, "/api/cj/orders") => {
            let order: serde_json::Value =
                serde_json::from_slice(&req.body).unwrap_or_else(|_| serde_json::json!({}));
            let config = agilang_framework_config::Config::load(project_root);
            let from_country_code = config.get("CJ_FROM_COUNTRY_CODE", "CN");
            let logistic_name = config.get("CJ_LOGISTIC_NAME", "");
            let cj_items = order
                .get("items")
                .and_then(|items| items.as_array())
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|item| {
                    item.get("cjSku")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        != ""
                })
                .collect::<Vec<_>>();
            if cj_items.is_empty() {
                Ok((
                    200,
                    serde_json::json!({
                        "ok": true,
                        "deferred": true,
                        "reason": "No CJ-linked products"
                    })
                    .to_string(),
                ))
            } else {
                if logistic_name.trim().is_empty() {
                    return Some((
                        400,
                        serde_json::json!({
                            "error": "CJ_LOGISTIC_NAME is not configured"
                        })
                        .to_string(),
                    ));
                }
                let mut enriched_order = order.clone();
                if enriched_order.get("customer").is_none()
                    || enriched_order
                        .get("customer")
                        .and_then(|v| v.as_object())
                        .is_none()
                {
                    enriched_order["customer"] = serde_json::json!({});
                }
                enriched_order["customer"]["fromCountryCode"] =
                    serde_json::Value::String(from_country_code);
                enriched_order["customer"]["logisticName"] =
                    serde_json::Value::String(logistic_name);
                match cj_request(
                    project_root,
                    "POST",
                    "/shopping/order/createOrderV2",
                    BTreeMap::new(),
                    Some(map_cj_order(&enriched_order, &cj_items)),
                ) {
                    Ok(resp) => {
                        let payload: serde_json::Value = serde_json::from_slice(&resp.body)
                            .unwrap_or_else(|_| serde_json::json!({}));
                        let data = payload.get("data").cloned().unwrap_or(payload);
                        let cj_order_id = data
                            .get("orderId")
                            .or_else(|| data.get("id"))
                            .or_else(|| data.get("orderNum"))
                            .cloned()
                            .unwrap_or(serde_json::Value::Null);
                        Ok((
                            resp.status,
                            serde_json::json!({
                                "ok": resp.status >= 200 && resp.status < 300,
                                "cjOrderId": cj_order_id,
                                "data": data
                            })
                            .to_string(),
                        ))
                    }
                    Err(err) => Err(err),
                }
            }
        }
        (HttpMethod::Get, "/api/cj/order") => {
            let id = req.query("id").unwrap_or("").to_string();
            let query = BTreeMap::from([("orderId".to_string(), vec![id])]);
            match cj_request(
                project_root,
                "GET",
                "/shopping/order/getOrderDetail",
                query,
                None,
            ) {
                Ok(resp) => {
                    let payload: serde_json::Value = serde_json::from_slice(&resp.body)
                        .unwrap_or_else(|_| serde_json::json!({}));
                    let data = payload.get("data").cloned().unwrap_or(payload);
                    Ok((
                        resp.status,
                        serde_json::json!({
                            "ok": resp.status >= 200 && resp.status < 300,
                            "data": data
                        })
                        .to_string(),
                    ))
                }
                Err(err) => Err(err),
            }
        }
        (HttpMethod::Post, "/api/cj/webhook") => Ok((
            200,
            serde_json::json!({
                "ok": true,
                "received": true,
                "provider": "CJdropshipping"
            })
            .to_string(),
        )),
        _ => Ok((
            404,
            serde_json::json!({ "error": "CJ route not found" }).to_string(),
        )),
    };

    Some(match response {
        Ok(result) => result,
        Err(err) => (502, serde_json::json!({ "error": err }).to_string()),
    })
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
    } else if expr.starts_with("Response.json") || expr.contains(".json(") {
        let mut status = 200u16;
        if expr.starts_with("Response.status(") {
            let status_expr = expr
                .trim_start_matches("Response.status(")
                .split(')')
                .next()
                .unwrap_or("")
                .trim();
            if let Ok(parsed) = status_expr.parse::<u16>() {
                status = parsed;
            }
        }
        if let Some(dict_start) = expr.find('{') {
            if let Some(dict_end) = expr.rfind('}') {
                let dict_content = &expr[dict_start + 1..dict_end];
                let mut entries = vec![];
                for pair in split_top_level(dict_content, ',') {
                    if let Some(colon_pos) = pair.find(':') {
                        let key = pair[..colon_pos]
                            .trim()
                            .trim_matches('"')
                            .trim_matches('\'')
                            .to_string();
                        let value = parse_json_value(&pair[colon_pos + 1..]);
                        entries.push((key, value));
                    }
                }
                return Ok(ControllerResult::Json(
                    status,
                    JsonValue::Object(entries).to_json(),
                ));
            }
        }
        Err(format!("Malformed Response.json expression: {}", expr))
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

            if let Some(upgrade_response) =
                try_handle_websocket_request(&mut stream, router, project_root, req)
            {
                let status = upgrade_response.status_code;
                if let Some(message) = upgrade_response.log_message {
                    eprintln!("{message}");
                }
                println!(
                    "{} {:?} {} {} {}ms",
                    now,
                    req.method,
                    path,
                    status,
                    start_time.elapsed().as_millis()
                );
                stream.flush().ok();
                return Ok(());
            }

            if let Some((status, json)) = native_cj_response(req, project_root) {
                (status, json, "application/json")
            } else if let Some(json) = builtin_api_response(path) {
                (200, json, "application/json")
            } else if let Some(response) = match handle_auth_request(req, view_engine, project_root)
            {
                Ok(response) => response,
                Err(error) => {
                    let response = Response::html(&format!(
                        "<h1>500 Internal Server Error</h1><pre>{}</pre>",
                        error
                    ));
                    write_framework_response(&mut stream, response)?;
                    return Ok(());
                }
            } {
                write_framework_response(&mut stream, response)?;
                println!(
                    "{} {:?} {} auth {}ms",
                    now,
                    req.method,
                    path,
                    start_time.elapsed().as_millis()
                );
                return Ok(());
            } else {
                let response =
                    controller_runtime::execute_request(router, view_engine, project_root, req);
                let status = response.status;
                write_framework_response(&mut stream, response)?;
                println!(
                    "{} {:?} {} {} {}ms",
                    now,
                    req.method,
                    path,
                    status,
                    start_time.elapsed().as_millis()
                );
                return Ok(());
            }
        }
    };

    let status_line = match status_code {
        200 => "HTTP/1.1 200 OK",
        201 => "HTTP/1.1 201 Created",
        400 => "HTTP/1.1 400 Bad Request",
        404 => "HTTP/1.1 404 Not Found",
        502 => "HTTP/1.1 502 Bad Gateway",
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

#[cfg(test)]
mod tests {
    use super::*;
    use agilang_framework_database::{DatabaseConfig, FrameworkDriver};
    use agilang_framework_migrations::{MigrationExecutor, MigrationFile};
    use agilang_project_generator::{generate_project, make_component};
    use std::fs;
    use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex, OnceLock};
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn builtin_status_apis_are_available() {
        let framework = builtin_api_response("/api/framework/status").unwrap();
        let runtime = builtin_api_response("/api/runtime/status").unwrap();
        let chain = builtin_api_response("/api/blockchain/status").unwrap();
        let auth = builtin_api_response("/api/auth/status").unwrap();
        let crud = builtin_api_response("/api/crud/status").unwrap();

        assert!(framework.contains("\"database_status\":\"AGIDB ready\""));
        assert!(framework.contains("\"webrtc_status\":\"signaling-only\""));
        assert!(framework.contains("\"webrtc_signaling\":true"));
        assert!(framework.contains("\"webrtc_peer_connection\":false"));
        assert!(runtime.contains("\"storage\":\"AGIDB\""));
        assert!(chain.contains("\"height\""));
        assert!(auth.contains("\"roles\":[\"user\",\"admin\"]"));
        assert!(crud.contains("\"entities\":[\"users\",\"projects\",\"tasks\"]"));
        assert!(builtin_api_response("/api/missing").is_none());
    }

    fn unique_test_root(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("agilang_framework_server_{name}_{stamp}"))
    }

    fn write_view(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn setup_auth_project(name: &str) -> (PathBuf, ViewEngine) {
        clear_auth_runtime_state();
        let root = unique_test_root(name);
        let views = root.join("resources/views");
        fs::create_dir_all(root.join("storage/database")).unwrap();
        write_view(
            &views.join("auth/login.ags"),
            r#"<!DOCTYPE html><html><body><h1>Login</h1><p>{{ error_message }}</p><form method="POST" action="/login"><input type="hidden" name="_csrf" value="{{ csrf_token }}"><input type="hidden" name="return_to" value="{{ return_to }}"><input name="email"><input name="password"><button>Login</button></form><a href="/register{{ return_to_query }}">Register</a></body></html>"#,
        );
        write_view(
            &views.join("auth/register.ags"),
            r#"<!DOCTYPE html><html><body><h1>Register</h1><p>{{ error_message }}</p><form method="POST" action="/register"><input type="hidden" name="_csrf" value="{{ csrf_token }}"><input type="hidden" name="return_to" value="{{ return_to }}"><input name="name"><input name="email"><input name="password"><button>Register</button></form><a href="/login{{ return_to_query }}">Login</a></body></html>"#,
        );
        write_view(
            &views.join("dashboard/user.ags"),
            r#"<!DOCTYPE html><html><body><h1>User Dashboard</h1><p>{{ user_name }}</p><form method="POST" action="/logout"><input type="hidden" name="_csrf" value="{{ csrf_token }}"><button>Logout</button></form></body></html>"#,
        );
        write_view(
            &views.join("dashboard/admin.ags"),
            r#"<!DOCTYPE html><html><body><h1>Admin Dashboard</h1><p>{{ user_name }}</p><form method="POST" action="/logout"><input type="hidden" name="_csrf" value="{{ csrf_token }}"><button>Logout</button></form></body></html>"#,
        );
        let engine = ViewEngine::new(views);
        (root, engine)
    }

    fn project_generation_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn request(
        method: HttpMethod,
        path: &str,
        cookie: Option<&str>,
        body: Option<&str>,
        ip_octet: u8,
    ) -> Request {
        let mut headers = HashMap::new();
        if let Some(cookie) = cookie {
            headers.insert("cookie".to_string(), cookie.to_string());
        }
        if body.is_some() {
            headers.insert(
                "content-type".to_string(),
                "application/x-www-form-urlencoded".to_string(),
            );
        }
        Request {
            method,
            path: path.to_string(),
            query: HashMap::new(),
            headers,
            body: body.unwrap_or_default().as_bytes().to_vec(),
            remote_addr: Some(SocketAddr::V4(SocketAddrV4::new(
                Ipv4Addr::new(127, 0, 0, ip_octet),
                4000 + ip_octet as u16,
            ))),
        }
    }

    fn response_text(response: &Response) -> String {
        String::from_utf8_lossy(&response.body).to_string()
    }

    fn encode_component(value: &str) -> String {
        let mut encoded = String::new();
        for byte in value.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    encoded.push(byte as char)
                }
                b' ' => encoded.push('+'),
                _ => encoded.push_str(&format!("%{byte:02X}")),
            }
        }
        encoded
    }

    fn form_body(entries: &[(&str, &str)]) -> String {
        entries
            .iter()
            .map(|(key, value)| format!("{}={}", encode_component(key), encode_component(value)))
            .collect::<Vec<_>>()
            .join("&")
    }

    fn invoke(project_root: &Path, view_engine: &ViewEngine, req: Request) -> Response {
        handle_auth_request(&req, view_engine, project_root)
            .unwrap()
            .expect("auth route should be handled")
    }

    fn invoke_app(project_root: &Path, req: Request) -> Response {
        let mut router = Router::new();
        router
            .load_routes_from_file(&project_root.join("routes/web.agi"))
            .unwrap();
        router
            .load_routes_from_file(&project_root.join("routes/api.agi"))
            .unwrap();
        let engine = ViewEngine::new(project_root.join("resources/views"));
        controller_runtime::execute_request(&router, &engine, project_root, &req)
    }

    fn json_request(
        method: HttpMethod,
        path: &str,
        cookie: Option<&str>,
        csrf: Option<&str>,
        body: &str,
        ip_octet: u8,
    ) -> Request {
        let mut headers = HashMap::new();
        headers.insert("content-type".to_string(), "application/json".to_string());
        if let Some(cookie) = cookie {
            headers.insert("cookie".to_string(), cookie.to_string());
        }
        if let Some(csrf) = csrf {
            headers.insert("x-csrf-token".to_string(), csrf.to_string());
        }
        Request {
            method,
            path: path.to_string(),
            query: HashMap::new(),
            headers,
            body: body.as_bytes().to_vec(),
            remote_addr: Some(SocketAddr::V4(SocketAddrV4::new(
                Ipv4Addr::new(127, 0, 0, ip_octet),
                5000 + ip_octet as u16,
            ))),
        }
    }

    fn query_request(
        method: HttpMethod,
        path: &str,
        query: &[(&str, &str)],
        cookie: Option<&str>,
        ip_octet: u8,
    ) -> Request {
        let mut req = request(method, path, cookie, None, ip_octet);
        req.query = query
            .iter()
            .map(|(key, value)| ((*key).to_string(), vec![(*value).to_string()]))
            .collect();
        req
    }

    fn cookie_from(response: &Response) -> String {
        response
            .headers
            .get("Set-Cookie")
            .and_then(|value| value.split(';').next())
            .unwrap_or_default()
            .to_string()
    }

    fn csrf_from(response: &Response) -> String {
        let body = response_text(response);
        let marker = r#"name="_csrf" value=""#;
        let start = body.find(marker).unwrap() + marker.len();
        let end = body[start..].find('"').unwrap() + start;
        body[start..end].to_string()
    }

    fn register_user(
        project_root: &Path,
        view_engine: &ViewEngine,
        ip_octet: u8,
        email: &str,
        password: &str,
        name: &str,
    ) -> String {
        let page = invoke(
            project_root,
            view_engine,
            request(HttpMethod::Get, "/register", None, None, ip_octet),
        );
        let guest_cookie = cookie_from(&page);
        let csrf = csrf_from(&page);
        let body = form_body(&[
            ("name", name),
            ("email", email),
            ("password", password),
            ("_csrf", &csrf),
        ]);
        let response = invoke(
            project_root,
            view_engine,
            request(
                HttpMethod::Post,
                "/register",
                Some(&guest_cookie),
                Some(&body),
                ip_octet,
            ),
        );
        assert_eq!(response.status, 302, "{}", response_text(&response));
        cookie_from(&response)
    }

    fn login_user(
        project_root: &Path,
        view_engine: &ViewEngine,
        ip_octet: u8,
        email: &str,
        password: &str,
    ) -> (String, String) {
        let page = invoke(
            project_root,
            view_engine,
            request(HttpMethod::Get, "/login", None, None, ip_octet),
        );
        let guest_cookie = cookie_from(&page);
        let csrf = csrf_from(&page);
        let body = form_body(&[("email", email), ("password", password), ("_csrf", &csrf)]);
        let response = invoke(
            project_root,
            view_engine,
            request(
                HttpMethod::Post,
                "/login",
                Some(&guest_cookie),
                Some(&body),
                ip_octet,
            ),
        );
        (guest_cookie, cookie_from(&response))
    }

    fn session_rows(project_root: &Path) -> Vec<DatabaseRow> {
        let mut conn = open_auth_db(project_root).unwrap();
        conn.query("SELECT * FROM sessions", &[]).unwrap()
    }

    fn user_rows(project_root: &Path) -> Vec<DatabaseRow> {
        let mut conn = open_auth_db(project_root).unwrap();
        conn.query("SELECT * FROM users", &[]).unwrap()
    }

    fn setup_generated_post_project(name: &str, driver: FrameworkDriver) -> PathBuf {
        let _lock = project_generation_lock().lock().unwrap();
        let root = unique_test_root(name);
        let root_str = root.to_string_lossy().to_string();
        generate_project(&root_str, "web").unwrap();

        let previous = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();
        make_component("resource", "Post", true).unwrap();
        std::env::set_current_dir(&previous).unwrap();

        let api_routes = r#"use Framework.Routing.Route
use App.Controllers.Api.HealthController
use App.Controllers.PostController

fn register_api() -> void:
    Route.group("/api", fn:
        Route.get("/health", HealthController.show)
        Route.get("/posts", PostController.index)
        Route.get("/posts/{id}", PostController.show)
        Route.post("/posts", PostController.store)
        Route.put("/posts/{id}", PostController.update)
        Route.delete("/posts/{id}", PostController.destroy)
        Route.post("/posts/{id}/restore", PostController.restore)
    )
"#;
        fs::write(root.join("routes/api.agi"), api_routes).unwrap();

        let model_path = root.join("app/Models/Post.agi");
        let model = fs::read_to_string(&model_path).unwrap();
        fs::write(&model_path, model.replace("hidden = []", "hidden = [\"deleted_at\"]")).unwrap();

        if driver == FrameworkDriver::Sqlite {
            let config_path = root.join("config/database.agi");
            let config = fs::read_to_string(&config_path).unwrap();
            fs::write(
                &config_path,
                config.replacen("\"default\": \"agidb\"", "\"default\": \"sqlite\"", 1),
            )
            .unwrap();
        }

        write_framework_manifest(&root).unwrap();
        fs::remove_file(root.join("app/Models/Post.agi")).unwrap();
        fs::remove_file(root.join("app/Requests/StorePostRequest.agi")).unwrap();
        fs::remove_file(root.join("app/Requests/UpdatePostRequest.agi")).unwrap();

        root
    }

    fn post_database_config(project_root: &Path, driver: FrameworkDriver) -> DatabaseConfig {
        match driver {
            FrameworkDriver::Agidb => DatabaseConfig::agidb(
                project_root
                    .join("storage/database/main.agidb")
                    .to_string_lossy(),
            ),
            FrameworkDriver::Sqlite => DatabaseConfig::sqlite(
                project_root
                    .join("storage/database/main.sqlite")
                    .to_string_lossy(),
            ),
            FrameworkDriver::Mysql => unreachable!("mysql lane is not part of this default test"),
        }
    }

    fn post_migration_file() -> MigrationFile {
        MigrationFile::new(
            "20260801_create_posts_table",
            vec![
                "CREATE TABLE posts (id INTEGER, name TEXT, created_at TEXT, updated_at TEXT, deleted_at TEXT)"
                    .to_string(),
            ],
            vec!["DROP TABLE IF EXISTS posts".to_string()],
            "create posts table",
        )
    }

    fn promote_user_to_admin(project_root: &Path, email: &str) {
        let mut conn = open_auth_db(project_root).unwrap();
        conn.execute(
            "UPDATE users SET role = ? WHERE email = ?",
            &[
                DatabaseValue::Text("admin".to_string()),
                DatabaseValue::Text(email.to_string()),
            ],
        )
        .unwrap();
    }

    fn run_generated_post_flow(driver: FrameworkDriver, name: &str) {
        clear_auth_runtime_state();
        let root = setup_generated_post_project(name, driver.clone());
        let config = post_database_config(&root, driver);
        let mut executor = MigrationExecutor::connect(&config).unwrap();
        executor.run_migrations(&[post_migration_file()], false).unwrap();

        let unauthorized = invoke_app(
            &root,
            json_request(HttpMethod::Post, "/api/posts", None, None, r#"{"name":"Nope"}"#, 60),
        );
        assert_eq!(unauthorized.status, 403, "{}", response_text(&unauthorized));

        let email = format!("{name}@example.com");
        let engine = ViewEngine::new(root.join("resources/views"));
        let auth_cookie = register_user(&root, &engine, 61, &email, "Password123!", "Poster");
        promote_user_to_admin(&root, &email);

        let dashboard = invoke(
            &root,
            &engine,
            request(HttpMethod::Get, "/dashboard", Some(&auth_cookie), None, 61),
        );
        let csrf = csrf_from(&dashboard);

        let csrf_denied = invoke_app(
            &root,
            json_request(
                HttpMethod::Post,
                "/api/posts",
                Some(&auth_cookie),
                None,
                r#"{"name":"Missing Csrf"}"#,
                61,
            ),
        );
        assert_eq!(csrf_denied.status, 403);

        let invalid = invoke_app(
            &root,
            json_request(HttpMethod::Post, "/api/posts", Some(&auth_cookie), Some(&csrf), "{}", 61),
        );
        assert_eq!(invalid.status, 422, "{}", response_text(&invalid));

        let create = invoke_app(
            &root,
            json_request(
                HttpMethod::Post,
                "/api/posts",
                Some(&auth_cookie),
                Some(&csrf),
                r#"{"name":"First Post","deleted_at":"2020-01-01T00:00:00Z"}"#,
                61,
            ),
        );
        assert_eq!(create.status, 201, "{}", response_text(&create));
        let created: serde_json::Value = serde_json::from_str(&response_text(&create)).unwrap();
        let post_id = created["data"]["id"].as_i64().unwrap();
        assert_eq!(created["data"]["name"], "First Post");
        assert!(created["data"].get("deleted_at").is_none());

        let list = invoke_app(&root, request(HttpMethod::Get, "/api/posts", None, None, 62));
        assert_eq!(list.status, 200);
        assert!(response_text(&list).contains("First Post"));

        let show = invoke_app(
            &root,
            request(HttpMethod::Get, &format!("/api/posts/{post_id}"), None, None, 62),
        );
        assert_eq!(show.status, 200);
        assert!(response_text(&show).contains("First Post"));
        assert!(!response_text(&show).contains("deleted_at"));

        let missing = invoke_app(&root, request(HttpMethod::Get, "/api/posts/999999", None, None, 62));
        assert_eq!(missing.status, 404);

        let update = invoke_app(
            &root,
            json_request(
                HttpMethod::Put,
                &format!("/api/posts/{post_id}"),
                Some(&auth_cookie),
                Some(&csrf),
                r#"{"name":"Updated Post","created_at":"tampered"}"#,
                61,
            ),
        );
        assert_eq!(update.status, 200, "{}", response_text(&update));
        assert!(response_text(&update).contains("Updated Post"));
        assert!(!response_text(&update).contains("tampered"));

        let delete = invoke_app(
            &root,
            json_request(
                HttpMethod::Delete,
                &format!("/api/posts/{post_id}"),
                Some(&auth_cookie),
                Some(&csrf),
                "{}",
                61,
            ),
        );
        assert_eq!(delete.status, 200);

        let after_delete = invoke_app(
            &root,
            request(HttpMethod::Get, &format!("/api/posts/{post_id}"), None, None, 62),
        );
        assert_eq!(after_delete.status, 404);

        let restore = invoke_app(
            &root,
            json_request(
                HttpMethod::Post,
                &format!("/api/posts/{post_id}/restore"),
                Some(&auth_cookie),
                Some(&csrf),
                "{}",
                61,
            ),
        );
        assert_eq!(restore.status, 200, "{}", response_text(&restore));

        let after_restore = invoke_app(
            &root,
            request(HttpMethod::Get, &format!("/api/posts/{post_id}"), None, None, 62),
        );
        assert_eq!(after_restore.status, 200);
        assert!(response_text(&after_restore).contains("Updated Post"));

        let after_restart = invoke_app(
            &root,
            request(HttpMethod::Get, &format!("/api/posts/{post_id}"), None, None, 62),
        );
        assert_eq!(after_restart.status, 200);

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn auth_lifecycle_is_enforced_end_to_end() {
        let (root, engine) = setup_auth_project("lifecycle");

        let unauth = invoke(
            &root,
            &engine,
            request(HttpMethod::Get, "/dashboard", None, None, 10),
        );
        assert_eq!(unauth.status, 302);
        assert_eq!(
            unauth.headers.get("Location").map(String::as_str),
            Some("/login?return_to=/dashboard")
        );

        let email = "lifecycle@example.com";
        let auth_cookie =
            register_user(&root, &engine, 11, email, "Password123!", "Lifecycle User");

        let me = invoke(
            &root,
            &engine,
            request(
                HttpMethod::Get,
                "/api/auth/me",
                Some(&auth_cookie),
                None,
                11,
            ),
        );
        assert_eq!(me.status, 200);
        assert!(response_text(&me).contains(email));

        let dashboard = invoke(
            &root,
            &engine,
            request(HttpMethod::Get, "/dashboard", Some(&auth_cookie), None, 11),
        );
        assert_eq!(dashboard.status, 200);
        let dashboard_html = response_text(&dashboard);
        assert!(dashboard_html.contains("User Dashboard"));
        let logout_csrf = csrf_from(&dashboard);

        let tampered_cookie = format!("{auth_cookie}x");
        let tampered = invoke(
            &root,
            &engine,
            request(
                HttpMethod::Get,
                "/dashboard",
                Some(&tampered_cookie),
                None,
                11,
            ),
        );
        assert_eq!(tampered.status, 302);
        assert_eq!(
            tampered.headers.get("Location").map(String::as_str),
            Some("/login?return_to=/dashboard")
        );

        let logout = invoke(
            &root,
            &engine,
            request(
                HttpMethod::Post,
                "/logout",
                Some(&auth_cookie),
                Some(&form_body(&[("_csrf", &logout_csrf)])),
                11,
            ),
        );
        assert_eq!(logout.status, 302);
        assert_eq!(
            logout.headers.get("Location").map(String::as_str),
            Some("/login")
        );
        assert!(logout
            .headers
            .get("Set-Cookie")
            .unwrap()
            .contains("Max-Age=0"));

        let revoked = invoke(
            &root,
            &engine,
            request(HttpMethod::Get, "/dashboard", Some(&auth_cookie), None, 11),
        );
        assert_eq!(revoked.status, 302);

        let (guest_cookie, login_cookie) = login_user(&root, &engine, 12, email, "Password123!");
        assert_ne!(guest_cookie, login_cookie);

        let fixed = invoke(
            &root,
            &engine,
            request(HttpMethod::Get, "/dashboard", Some(&guest_cookie), None, 12),
        );
        assert_eq!(fixed.status, 302);

        let after_login = invoke(
            &root,
            &engine,
            request(HttpMethod::Get, "/dashboard", Some(&login_cookie), None, 12),
        );
        assert_eq!(after_login.status, 200);

        let after_restart_engine = ViewEngine::new(root.join("resources/views"));
        let after_restart = invoke(
            &root,
            &after_restart_engine,
            request(HttpMethod::Get, "/dashboard", Some(&login_cookie), None, 12),
        );
        assert_eq!(after_restart.status, 200);
        assert_eq!(user_rows(&root).len(), 1);
        assert_eq!(
            session_rows(&root)
                .iter()
                .filter(|row| {
                    row_integer(row, "user_id").is_some()
                        || !row_text(row, "user_id").unwrap_or_default().is_empty()
                })
                .count(),
            1
        );

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn csrf_is_enforced_for_cookie_authenticated_state_changes() {
        let (root, engine) = setup_auth_project("csrf");
        let register_page = invoke(
            &root,
            &engine,
            request(HttpMethod::Get, "/register", None, None, 20),
        );
        let guest_cookie = cookie_from(&register_page);

        let register_bad = invoke(
            &root,
            &engine,
            request(
                HttpMethod::Post,
                "/register",
                Some(&guest_cookie),
                Some(&form_body(&[
                    ("name", "Nope"),
                    ("email", "csrf1@example.com"),
                    ("password", "Password123!"),
                    ("_csrf", "wrong"),
                ])),
                20,
            ),
        );
        assert_eq!(register_bad.status, 403);

        let auth_cookie = register_user(
            &root,
            &engine,
            21,
            "csrf2@example.com",
            "Password123!",
            "Csrf User",
        );

        let login_page = invoke(
            &root,
            &engine,
            request(HttpMethod::Get, "/login", None, None, 22),
        );
        let login_guest_cookie = cookie_from(&login_page);
        let login_bad = invoke(
            &root,
            &engine,
            request(
                HttpMethod::Post,
                "/login",
                Some(&login_guest_cookie),
                Some(&form_body(&[
                    ("email", "csrf2@example.com"),
                    ("password", "Password123!"),
                    ("_csrf", "wrong"),
                ])),
                22,
            ),
        );
        assert_eq!(login_bad.status, 403);

        let logout_bad = invoke(
            &root,
            &engine,
            request(
                HttpMethod::Post,
                "/logout",
                Some(&auth_cookie),
                Some(&form_body(&[("_csrf", "wrong")])),
                21,
            ),
        );
        assert_eq!(logout_bad.status, 403);

        let api_logout_bad = invoke(
            &root,
            &engine,
            request(
                HttpMethod::Post,
                "/api/auth/logout",
                Some(&auth_cookie),
                Some(&form_body(&[("_csrf", "wrong")])),
                21,
            ),
        );
        assert_eq!(api_logout_bad.status, 403);

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn rate_limits_and_duplicate_registration_are_enforced() {
        let (root, engine) = setup_auth_project("limits");

        for index in 0..RATE_LIMIT_REGISTER_MAX {
            let email = format!("limit{index}@example.com");
            let response_cookie =
                register_user(&root, &engine, 30, &email, "Password123!", "Limit User");
            assert!(!response_cookie.is_empty());
        }
        let extra_page = invoke(
            &root,
            &engine,
            request(HttpMethod::Get, "/register", None, None, 30),
        );
        let extra_cookie = cookie_from(&extra_page);
        let extra_csrf = csrf_from(&extra_page);
        let extra_register = invoke(
            &root,
            &engine,
            request(
                HttpMethod::Post,
                "/register",
                Some(&extra_cookie),
                Some(&form_body(&[
                    ("name", "Extra"),
                    ("email", "extra@example.com"),
                    ("password", "Password123!"),
                    ("_csrf", &extra_csrf),
                ])),
                30,
            ),
        );
        assert_eq!(extra_register.status, 429);

        let (login_root, login_engine) = setup_auth_project("login_limits");
        let _cookie = register_user(
            &login_root,
            &login_engine,
            31,
            "rate-login@example.com",
            "Password123!",
            "Rate Login",
        );
        for _ in 0..RATE_LIMIT_LOGIN_MAX {
            let page = invoke(
                &login_root,
                &login_engine,
                request(HttpMethod::Get, "/login", None, None, 32),
            );
            let guest_cookie = cookie_from(&page);
            let csrf = csrf_from(&page);
            let failed = invoke(
                &login_root,
                &login_engine,
                request(
                    HttpMethod::Post,
                    "/login",
                    Some(&guest_cookie),
                    Some(&form_body(&[
                        ("email", "rate-login@example.com"),
                        ("password", "wrongpass"),
                        ("_csrf", &csrf),
                    ])),
                    32,
                ),
            );
            assert_eq!(failed.status, 401);
        }
        let page = invoke(
            &login_root,
            &login_engine,
            request(HttpMethod::Get, "/login", None, None, 32),
        );
        let guest_cookie = cookie_from(&page);
        let csrf = csrf_from(&page);
        let throttled = invoke(
            &login_root,
            &login_engine,
            request(
                HttpMethod::Post,
                "/login",
                Some(&guest_cookie),
                Some(&form_body(&[
                    ("email", "rate-login@example.com"),
                    ("password", "wrongpass"),
                    ("_csrf", &csrf),
                ])),
                32,
            ),
        );
        assert_eq!(throttled.status, 429);

        let (concurrent_root, concurrent_engine) = setup_auth_project("concurrent_register");
        let shared_root = Arc::new(concurrent_root);
        let email = "race@example.com".to_string();
        let mut registration_contexts = Vec::new();
        for ip_octet in [40u8, 41u8] {
            let page = invoke(
                shared_root.as_ref(),
                &concurrent_engine,
                request(HttpMethod::Get, "/register", None, None, ip_octet),
            );
            registration_contexts.push((ip_octet, cookie_from(&page), csrf_from(&page)));
        }
        let mut handles = Vec::new();
        for (ip_octet, guest_cookie, csrf) in registration_contexts {
            let root = Arc::clone(&shared_root);
            let email = email.clone();
            handles.push(thread::spawn(move || {
                let engine = ViewEngine::new(root.join("resources/views"));
                invoke(
                    &root,
                    &engine,
                    request(
                        HttpMethod::Post,
                        "/register",
                        Some(&guest_cookie),
                        Some(&form_body(&[
                            ("name", "Race"),
                            ("email", &email),
                            ("password", "Password123!"),
                            ("_csrf", &csrf),
                        ])),
                        ip_octet,
                    ),
                )
                .status
            }));
        }
        let statuses = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>();
        assert!(statuses.contains(&302), "{statuses:?}");
        assert!(statuses.contains(&409), "{statuses:?}");
        let users = user_rows(&shared_root);
        let race_users = users
            .iter()
            .filter(|row| row_text(row, "email").unwrap_or_default() == "race@example.com")
            .count();
        assert_eq!(race_users, 1);

        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(login_root).ok();
        fs::remove_dir_all(shared_root.as_ref()).ok();
    }

    #[test]
    fn authorization_rotation_cleanup_and_concurrent_logout_are_enforced() {
        let (root, engine) = setup_auth_project("authorization");
        let user_cookie = register_user(
            &root,
            &engine,
            50,
            "roles@example.com",
            "Password123!",
            "Role User",
        );

        let admin_denied = invoke(
            &root,
            &engine,
            request(
                HttpMethod::Get,
                "/dashboard/admin",
                Some(&user_cookie),
                None,
                50,
            ),
        );
        assert_eq!(admin_denied.status, 403);

        let mut conn = open_auth_db(&root).unwrap();
        conn.execute(
            "UPDATE users SET role = ? WHERE email = ?",
            &[
                DatabaseValue::Text("admin".to_string()),
                DatabaseValue::Text("roles@example.com".to_string()),
            ],
        )
        .unwrap();

        let admin_ok = invoke(
            &root,
            &engine,
            request(
                HttpMethod::Get,
                "/dashboard/admin",
                Some(&user_cookie),
                None,
                50,
            ),
        );
        assert_eq!(admin_ok.status, 200);
        assert!(response_text(&admin_ok).contains("Admin Dashboard"));

        let mut auth_cookies = Vec::new();
        for ip_octet in 51..=56 {
            let (_, auth_cookie) = login_user(
                &root,
                &engine,
                ip_octet,
                "roles@example.com",
                "Password123!",
            );
            auth_cookies.push(auth_cookie);
        }
        let admin_user_id = user_rows(&root)
            .into_iter()
            .find(|row| row_text(row, "email").unwrap_or_default() == "roles@example.com")
            .and_then(|row| row_integer(&row, "id").map(|value| value.to_string()))
            .unwrap();
        let user_sessions = session_rows(&root)
            .into_iter()
            .filter(|row| row_text(row, "user_id").unwrap_or_default() == admin_user_id)
            .count();
        assert_eq!(user_sessions, MAX_ACTIVE_USER_SESSIONS);

        let oldest_cookie = auth_cookies.first().unwrap().clone();
        let newest_cookie = auth_cookies.last().unwrap().clone();
        let oldest_access = invoke(
            &root,
            &engine,
            request(
                HttpMethod::Get,
                "/dashboard",
                Some(&oldest_cookie),
                None,
                51,
            ),
        );
        assert_eq!(oldest_access.status, 302);
        let newest_access = invoke(
            &root,
            &engine,
            request(
                HttpMethod::Get,
                "/dashboard",
                Some(&newest_cookie),
                None,
                56,
            ),
        );
        assert_eq!(newest_access.status, 200);

        let dashboard = invoke(
            &root,
            &engine,
            request(
                HttpMethod::Get,
                "/dashboard",
                Some(&newest_cookie),
                None,
                56,
            ),
        );
        let csrf = csrf_from(&dashboard);
        let first_logout = invoke(
            &root,
            &engine,
            request(
                HttpMethod::Post,
                "/logout",
                Some(&newest_cookie),
                Some(&form_body(&[("_csrf", &csrf)])),
                56,
            ),
        );
        assert_eq!(first_logout.status, 302);

        let second_logout = invoke(
            &root,
            &engine,
            request(
                HttpMethod::Post,
                "/logout",
                Some(&newest_cookie),
                Some(&form_body(&[("_csrf", &csrf)])),
                56,
            ),
        );
        assert_eq!(second_logout.status, 302);
        assert!(second_logout
            .headers
            .get("Set-Cookie")
            .unwrap()
            .contains("Max-Age=0"));

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn login_and_register_preserve_safe_return_to() {
        let (root, engine) = setup_auth_project("return_to");

        let login_page = invoke(
            &root,
            &engine,
            query_request(
                HttpMethod::Get,
                "/login",
                &[("return_to", "/?meeting=246945224&host=meeting-246945224-video-alpha-queral-local")],
                None,
                41,
            ),
        );
        assert_eq!(login_page.status, 200);
        let login_html = response_text(&login_page);
        assert!(login_html.contains(r#"name="return_to" value="/?meeting=246945224&host=meeting-246945224-video-alpha-queral-local""#));
        assert!(login_html.contains("/register?return_to=/%3Fmeeting%3D246945224%26host%3Dmeeting-246945224-video-alpha-queral-local"));

        let register_page = invoke(
            &root,
            &engine,
            query_request(
                HttpMethod::Get,
                "/register",
                &[("return_to", "/?meeting=246945224&host=meeting-246945224-video-alpha-queral-local")],
                None,
                42,
            ),
        );
        let register_cookie = cookie_from(&register_page);
        let register_csrf = csrf_from(&register_page);
        let registered = invoke(
            &root,
            &engine,
            request(
                HttpMethod::Post,
                "/register",
                Some(&register_cookie),
                Some(&form_body(&[
                    ("_csrf", &register_csrf),
                    ("return_to", "/?meeting=246945224&host=meeting-246945224-video-alpha-queral-local"),
                    ("name", "Meeting Guest"),
                    ("email", "meeting-guest@example.com"),
                    ("password", "Password123!"),
                ])),
                42,
            ),
        );
        assert_eq!(registered.status, 302);
        assert_eq!(
            registered.headers.get("Location").map(String::as_str),
            Some("/?meeting=246945224&host=meeting-246945224-video-alpha-queral-local")
        );

        let invalid = invoke(
            &root,
            &engine,
            query_request(
                HttpMethod::Get,
                "/dashboard",
                &[("return_to", "https://evil.example/steal")],
                None,
                43,
            ),
        );
        assert_eq!(invalid.status, 302);
        assert_eq!(
            invalid.headers.get("Location").map(String::as_str),
            Some("/login?return_to=/dashboard")
        );

        let localhost_login = invoke(
            &root,
            &engine,
            Request {
                method: HttpMethod::Get,
                path: "/login".to_string(),
                query: HashMap::new(),
                headers: HashMap::from([("host".to_string(), "localhost:8081".to_string())]),
                body: Vec::new(),
                remote_addr: Some(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 44))),
            },
        );
        let localhost_cookie = localhost_login.headers.get("Set-Cookie").cloned().unwrap_or_default();
        assert!(localhost_cookie.contains("SameSite=Lax"));
        assert!(!localhost_cookie.contains("Secure"));

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn login_recovers_from_legacy_duplicate_email_rows() {
        let (root, engine) = setup_auth_project("duplicate_email_rows");

        let register_page = invoke(
            &root,
            &engine,
            request(HttpMethod::Get, "/register", None, None, 51),
        );
        let register_cookie = cookie_from(&register_page);
        let register_csrf = csrf_from(&register_page);

        let registered = invoke(
            &root,
            &engine,
            request(
                HttpMethod::Post,
                "/register",
                Some(&register_cookie),
                Some(&form_body(&[
                    ("_csrf", &register_csrf),
                    ("name", "Duplicate Session User"),
                    ("email", "duplicate.session@example.com"),
                    ("password", "Password123!"),
                ])),
                51,
            ),
        );
        assert_eq!(registered.status, 302);

        let mut conn = open_auth_db(&root).unwrap();
        let legacy_hash = Argon2idPasswordHasher::default()
            .hash(b"OlderPassword456!")
            .unwrap();
        conn.execute(
            "INSERT INTO users (name, email, password_hash, role, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            &[
                DatabaseValue::Text("Legacy Duplicate".to_string()),
                DatabaseValue::Text("duplicate.session@example.com".to_string()),
                DatabaseValue::Text(legacy_hash),
                DatabaseValue::Text("user".to_string()),
                DatabaseValue::Integer(1),
                DatabaseValue::Integer(1),
            ],
        )
        .unwrap();

        let (_guest_cookie, login_cookie) = login_user(
            &root,
            &engine,
            52,
            "duplicate.session@example.com",
            "Password123!",
        );

        let me = invoke(
            &root,
            &engine,
            request(HttpMethod::Get, "/api/auth/me", Some(&login_cookie), None, 52),
        );
        assert_eq!(me.status, 200);
        assert!(response_text(&me).contains("duplicate.session@example.com"));

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn meeting_login_flow_persists_session_in_agidb() {
        let (root, engine) = setup_auth_project("meeting_login_flow");

        let auth_cookie = register_user(
            &root,
            &engine,
            53,
            "meeting.flow@example.com",
            "Password123!",
            "Meeting Flow User",
        );

        let me_after_register = invoke(
            &root,
            &engine,
            request(HttpMethod::Get, "/api/auth/me", Some(&auth_cookie), None, 53),
        );
        assert_eq!(me_after_register.status, 200);
        assert!(response_text(&me_after_register).contains("meeting.flow@example.com"));

        let session_count_after_register = session_rows(&root)
            .iter()
            .filter(|row| {
                row_integer(row, "user_id").is_some()
                    || !row_text(row, "user_id").unwrap_or_default().is_empty()
            })
            .count();
        assert!(session_count_after_register >= 1);

        let login_page = invoke(
            &root,
            &engine,
            query_request(
                HttpMethod::Get,
                "/login",
                &[("return_to", "/?meeting=246945224")],
                None,
                54,
            ),
        );
        let guest_cookie = cookie_from(&login_page);
        let csrf = csrf_from(&login_page);
        let login_response = invoke(
            &root,
            &engine,
            request(
                HttpMethod::Post,
                "/login",
                Some(&guest_cookie),
                Some(&form_body(&[
                    ("email", "meeting.flow@example.com"),
                    ("password", "Password123!"),
                    ("return_to", "/?meeting=246945224"),
                    ("_csrf", &csrf),
                ])),
                54,
            ),
        );
        assert_eq!(login_response.status, 302);
        assert_eq!(
            login_response.headers.get("Location").map(String::as_str),
            Some("/?meeting=246945224")
        );

        let login_cookie = cookie_from(&login_response);
        let me_after_login = invoke(
            &root,
            &engine,
            request(HttpMethod::Get, "/api/auth/me", Some(&login_cookie), None, 54),
        );
        assert_eq!(me_after_login.status, 200);
        assert!(response_text(&me_after_login).contains("meeting.flow@example.com"));

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn resolved_auth_db_path_uses_project_relative_env_file() {
        let root = unique_test_root("auth_db_relative_env");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join(".env"), "AGIDB_DATABASE=storage/database/main.agidb\n").unwrap();

        let resolved = resolved_auth_db_path(&root);
        assert_eq!(
            resolved,
            root.join("storage/database/main.agidb")
                .to_string_lossy()
                .to_string()
        );

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn resolved_auth_db_path_uses_project_relative_agilang_toml_database_path() {
        let root = unique_test_root("auth_db_toml");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("agilang.toml"),
            "[database]\npath = \"storage/database/main.agidb\"\n",
        )
        .unwrap();

        let resolved = resolved_auth_db_path(&root);
        assert_eq!(
            resolved,
            root.join("storage/database/main.agidb")
                .to_string_lossy()
                .to_string()
        );

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn generated_post_resource_executes_end_to_end_on_agidb() {
        run_generated_post_flow(FrameworkDriver::Agidb, "generated_post_agidb");
    }

    #[test]
    fn generated_post_resource_executes_end_to_end_on_sqlite() {
        run_generated_post_flow(FrameworkDriver::Sqlite, "generated_post_sqlite");
    }

    #[test]
    fn webrtc_signaling_requires_authentication() {
        let (root, _) = setup_auth_project("webrtc_auth");

        let status = invoke_app(
            &root,
            request(HttpMethod::Get, "/api/webrtc/status", None, None, 70),
        );
        assert_eq!(status.status, 200);
        assert!(response_text(&status).contains("\"signaling\":true"));
        assert!(response_text(&status).contains("\"peer_connection\":false"));
        assert!(response_text(&status).contains("\"active_peers\":0"));

        let unauthorized_register = invoke_app(
            &root,
            json_request(
                HttpMethod::Post,
                "/api/webrtc/register",
                None,
                None,
                r#"{"peer_id":"alice-peer"}"#,
                70,
            ),
        );
        assert_eq!(unauthorized_register.status, 401);

        let unauthorized_poll = invoke_app(
            &root,
            query_request(
                HttpMethod::Get,
                "/api/webrtc/poll",
                &[("peer_id", "alice-peer")],
                None,
                70,
            ),
        );
        assert_eq!(unauthorized_poll.status, 401);

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn webrtc_signaling_enforces_csrf_and_delivers_messages() {
        let (root, engine) = setup_auth_project("webrtc_signal");
        let alice_cookie = register_user(
            &root,
            &engine,
            71,
            "alice-webrtc@example.com",
            "Password123!",
            "Alice",
        );
        let bob_cookie = register_user(
            &root,
            &engine,
            72,
            "bob-webrtc@example.com",
            "Password123!",
            "Bob",
        );

        let alice_dashboard = invoke(
            &root,
            &engine,
            request(HttpMethod::Get, "/dashboard", Some(&alice_cookie), None, 71),
        );
        let alice_csrf = csrf_from(&alice_dashboard);
        let bob_dashboard = invoke(
            &root,
            &engine,
            request(HttpMethod::Get, "/dashboard", Some(&bob_cookie), None, 72),
        );
        let bob_csrf = csrf_from(&bob_dashboard);

        let csrf_denied = invoke_app(
            &root,
            json_request(
                HttpMethod::Post,
                "/api/webrtc/signal",
                Some(&alice_cookie),
                Some("wrong"),
                r#"{"from":"alice-peer","to":"bob-peer","kind":"offer","payload":"offer-sdp"}"#,
                71,
            ),
        );
        assert_eq!(csrf_denied.status, 403);
        assert!(response_text(&csrf_denied).contains("Invalid CSRF token."));

        let alice_register = invoke_app(
            &root,
            json_request(
                HttpMethod::Post,
                "/api/webrtc/register",
                Some(&alice_cookie),
                Some(&alice_csrf),
                r#"{"peer_id":"alice-peer"}"#,
                71,
            ),
        );
        assert_eq!(alice_register.status, 200);
        assert!(response_text(&alice_register).contains("\"registered\":true"));
        assert!(response_text(&alice_register).contains("\"active_peers\":1"));

        let bob_register = invoke_app(
            &root,
            json_request(
                HttpMethod::Post,
                "/api/webrtc/register",
                Some(&bob_cookie),
                Some(&bob_csrf),
                r#"{"peer_id":"bob-peer"}"#,
                72,
            ),
        );
        assert_eq!(bob_register.status, 200);
        assert!(response_text(&bob_register).contains("\"active_peers\":2"));

        let duplicate_claim = invoke_app(
            &root,
            json_request(
                HttpMethod::Post,
                "/api/webrtc/register",
                Some(&bob_cookie),
                Some(&bob_csrf),
                r#"{"peer_id":"alice-peer"}"#,
                72,
            ),
        );
        assert_eq!(duplicate_claim.status, 409);
        assert!(response_text(&duplicate_claim).contains("already registered"));

        let offer = invoke_app(
            &root,
            json_request(
                HttpMethod::Post,
                "/api/webrtc/signal",
                Some(&alice_cookie),
                Some(&alice_csrf),
                r#"{"from":"alice-peer","to":"bob-peer","kind":"offer","payload":"offer-sdp"}"#,
                71,
            ),
        );
        assert_eq!(offer.status, 202);
        assert!(response_text(&offer).contains("bob-webrtc@example.com"));

        let candidate = invoke_app(
            &root,
            json_request(
                HttpMethod::Post,
                "/api/webrtc/signal",
                Some(&alice_cookie),
                Some(&alice_csrf),
                r#"{"from":"alice-peer","to":"bob-peer","kind":"ice-candidate","payload":"candidate-1"}"#,
                71,
            ),
        );
        assert_eq!(candidate.status, 202);

        let spoof_attempt = invoke_app(
            &root,
            json_request(
                HttpMethod::Post,
                "/api/webrtc/signal",
                Some(&alice_cookie),
                Some(&alice_csrf),
                r#"{"from":"bob-peer","to":"alice-peer","kind":"offer","payload":"stolen"}"#,
                71,
            ),
        );
        assert_eq!(spoof_attempt.status, 403);
        assert!(response_text(&spoof_attempt).contains("ownership mismatch"));

        let missing_target = invoke_app(
            &root,
            json_request(
                HttpMethod::Post,
                "/api/webrtc/signal",
                Some(&alice_cookie),
                Some(&alice_csrf),
                r#"{"from":"alice-peer","to":"ghost-peer","kind":"offer","payload":"ghost"}"#,
                71,
            ),
        );
        assert_eq!(missing_target.status, 404);
        assert!(response_text(&missing_target).contains("destination peer"));

        let first_poll = invoke_app(
            &root,
            query_request(
                HttpMethod::Get,
                "/api/webrtc/poll",
                &[("peer_id", "bob-peer")],
                Some(&bob_cookie),
                72,
            ),
        );
        assert_eq!(first_poll.status, 200);
        let first_body = response_text(&first_poll);
        assert!(first_body.contains("\"kind\":\"offer\""));
        assert!(first_body.contains("\"payload\":\"offer-sdp\""));

        let second_poll = invoke_app(
            &root,
            query_request(
                HttpMethod::Get,
                "/api/webrtc/poll",
                &[("peer_id", "bob-peer")],
                Some(&bob_cookie),
                72,
            ),
        );
        assert_eq!(second_poll.status, 200);
        let second_body = response_text(&second_poll);
        assert!(second_body.contains("\"kind\":\"ice-candidate\""));
        assert!(second_body.contains("\"payload\":\"candidate-1\""));

        let empty_poll = invoke_app(
            &root,
            query_request(
                HttpMethod::Get,
                "/api/webrtc/poll",
                &[("peer_id", "bob-peer")],
                Some(&bob_cookie),
                72,
            ),
        );
        assert_eq!(empty_poll.status, 200);
        assert!(response_text(&empty_poll).contains("\"message\":null"));

        let unauthorized_peer_poll = invoke_app(
            &root,
            query_request(
                HttpMethod::Get,
                "/api/webrtc/poll",
                &[("peer_id", "alice-peer")],
                Some(&bob_cookie),
                72,
            ),
        );
        assert_eq!(unauthorized_peer_poll.status, 403);
        assert!(response_text(&unauthorized_peer_poll).contains("ownership mismatch"));

        let alice_logout = invoke(
            &root,
            &engine,
            request(
                HttpMethod::Post,
                "/logout",
                Some(&alice_cookie),
                Some(&form_body(&[("_csrf", &alice_csrf)])),
                71,
            ),
        );
        assert_eq!(alice_logout.status, 302);

        let revoked_signal = invoke_app(
            &root,
            json_request(
                HttpMethod::Post,
                "/api/webrtc/signal",
                Some(&alice_cookie),
                Some(&alice_csrf),
                r#"{"from":"alice-peer","to":"bob-peer","kind":"offer","payload":"after-logout"}"#,
                71,
            ),
        );
        assert_eq!(revoked_signal.status, 401);

        let status_after_logout = invoke_app(
            &root,
            request(HttpMethod::Get, "/api/webrtc/status", None, None, 70),
        );
        assert_eq!(status_after_logout.status, 200);
        assert!(response_text(&status_after_logout).contains("\"active_peers\":1"));

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn webrtc_meeting_registry_resolves_host_and_cleans_up_members() {
        let (root, engine) = setup_auth_project("webrtc_meeting_registry");
        let host_cookie = register_user(
            &root,
            &engine,
            73,
            "host-webrtc@example.com",
            "Password123!",
            "Host",
        );
        let guest_cookie = register_user(
            &root,
            &engine,
            74,
            "guest-webrtc@example.com",
            "Password123!",
            "Guest",
        );

        let host_csrf = csrf_from(&invoke(
            &root,
            &engine,
            request(HttpMethod::Get, "/dashboard", Some(&host_cookie), None, 73),
        ));
        let guest_csrf = csrf_from(&invoke(
            &root,
            &engine,
            request(HttpMethod::Get, "/dashboard", Some(&guest_cookie), None, 74),
        ));

        let host_register = invoke_app(
            &root,
            json_request(
                HttpMethod::Post,
                "/api/webrtc/register",
                Some(&host_cookie),
                Some(&host_csrf),
                r#"{"peer_id":"meeting-123-host","meeting_id":"meeting-123","role":"host"}"#,
                73,
            ),
        );
        assert_eq!(host_register.status, 200);
        assert!(response_text(&host_register).contains("\"host_peer_id\":\"meeting-123-host\""));
        assert!(response_text(&host_register).contains("\"member_count\":1"));

        let guest_register = invoke_app(
            &root,
            json_request(
                HttpMethod::Post,
                "/api/webrtc/register",
                Some(&guest_cookie),
                Some(&guest_csrf),
                r#"{"peer_id":"meeting-123-guest","meeting_id":"meeting-123","role":"participant"}"#,
                74,
            ),
        );
        assert_eq!(guest_register.status, 200);
        assert!(response_text(&guest_register).contains("\"host_peer_id\":\"meeting-123-host\""));
        assert!(response_text(&guest_register).contains("\"member_count\":2"));

        let meeting_lookup = invoke_app(
            &root,
            query_request(
                HttpMethod::Get,
                "/api/webrtc/meeting",
                &[("meeting_id", "meeting-123")],
                Some(&guest_cookie),
                74,
            ),
        );
        assert_eq!(meeting_lookup.status, 200);
        assert!(response_text(&meeting_lookup).contains("\"host_peer_id\":\"meeting-123-host\""));
        assert!(response_text(&meeting_lookup).contains("\"member_count\":2"));

        let logout = invoke(
            &root,
            &engine,
            request(
                HttpMethod::Post,
                "/logout",
                Some(&host_cookie),
                Some(&form_body(&[("_csrf", &host_csrf)])),
                73,
            ),
        );
        assert_eq!(logout.status, 302);

        let meeting_after_logout = invoke_app(
            &root,
            query_request(
                HttpMethod::Get,
                "/api/webrtc/meeting",
                &[("meeting_id", "meeting-123")],
                Some(&guest_cookie),
                74,
            ),
        );
        assert_eq!(meeting_after_logout.status, 200);
        assert!(response_text(&meeting_after_logout).contains("\"host_peer_id\":\"meeting-123-guest\""));
        assert!(response_text(&meeting_after_logout).contains("\"member_count\":1"));

        fs::remove_dir_all(root).ok();
    }
}

struct WebSocketUpgradeResult {
    status_code: u16,
    log_message: Option<String>,
}

fn try_handle_websocket_request(
    stream: &mut TcpStream,
    router: &Router,
    project_root: &Path,
    req: &Request,
) -> Option<WebSocketUpgradeResult> {
    let is_websocket = req
        .headers
        .get("upgrade")
        .or_else(|| req.headers.get("Upgrade"))
        .map(|v| v.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false);
    if !is_websocket {
        return None;
    }

    let path = req.path();
    let Some(route_match) = router.match_websocket_route(path) else {
        let response = build_text_response(
            404,
            "text/plain; charset=utf-8",
            "WebSocket route not defined.".to_string(),
        );
        let _ = write_framework_response(stream, response);
        return Some(WebSocketUpgradeResult {
            status_code: 404,
            log_message: None,
        });
    };

    let status_code = match dispatch_websocket_route(project_root, req, route_match, &mut *stream) {
        Ok(()) => 101,
        Err(WebSocketError::BadRequest(message)) => {
            let response = build_text_response(400, "text/plain; charset=utf-8", message);
            let _ = write_framework_response(stream, response);
            400
        }
        Err(WebSocketError::Protocol(message)) => {
            let response = build_text_response(400, "text/plain; charset=utf-8", message);
            let _ = write_framework_response(stream, response);
            400
        }
        Err(error) => {
            let response =
                build_text_response(500, "text/plain; charset=utf-8", error.to_string());
            let _ = write_framework_response(stream, response);
            500
        }
    };

    Some(WebSocketUpgradeResult {
        status_code,
        log_message: (status_code == 101).then_some(format!("websocket upgraded for {path}")),
    })
}

fn dispatch_websocket_route(
    _project_root: &Path,
    req: &Request,
    route_match: agilang_framework_routing::RouteMatch<'_>,
    stream: &mut TcpStream,
) -> Result<(), WebSocketError> {
    let route = route_match.route;
    let owned_stream = stream.try_clone()?;
    let mut socket = accept_websocket(owned_stream, &req.headers)?;
    match (route.controller.as_str(), route.action.as_str()) {
        ("ChatController", "echo") | ("WebSocketController", "echo") => {
            websocket_echo_handler(&mut socket)
        }
        _ => Err(WebSocketError::Protocol(format!(
            "WebSocket handler `{}.{}` is not registered",
            route.controller, route.action
        ))),
    }
}

fn websocket_echo_handler(
    socket: &mut WebSocketConnection<TcpStream>,
) -> Result<(), WebSocketError> {
    loop {
        let frame = socket.read_message()?;
        match frame.opcode {
            WebSocketOpcode::Text => socket.send_text(frame.text_value()?)?,
            WebSocketOpcode::Binary => socket.send_binary(&frame.payload)?,
            WebSocketOpcode::Close => return Ok(()),
            WebSocketOpcode::Continuation | WebSocketOpcode::Ping | WebSocketOpcode::Pong => {
                return Err(WebSocketError::Protocol(
                    "unexpected internal WebSocket frame state".to_string(),
                ))
            }
        }
    }
}
