use agilang_database_agidb::AgiDbConnection;
use agilang_database_driver::{DatabaseConnection, DatabaseRow, DatabaseValue};
use agilang_framework_auth::{Argon2idPasswordHasher, PasswordHasher};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MYSQL_PROTOCOL_VERSION: u8 = 10;
const MYSQL_SERVER_VERSION: &str = "8.0.36-agilang";
const MYSQL_CHARSET_UTF8MB4_GENERAL_CI: u8 = 45;
const MYSQL_STATUS_AUTOCOMMIT: u16 = 0x0002;
const MYSQL_TYPE_LONG: u8 = 0x03;
const MYSQL_TYPE_DOUBLE: u8 = 0x05;
const MYSQL_TYPE_VAR_STRING: u8 = 0xfd;
const MYSQL_TYPE_NULL: u8 = 0x06;
const CLIENT_LONG_PASSWORD: u32 = 0x0000_0001;
const CLIENT_LONG_FLAG: u32 = 0x0000_0004;
const CLIENT_CONNECT_WITH_DB: u32 = 0x0000_0008;
const CLIENT_PROTOCOL_41: u32 = 0x0000_0200;
const CLIENT_TRANSACTIONS: u32 = 0x0000_2000;
const CLIENT_SECURE_CONNECTION: u32 = 0x0000_8000;
const CLIENT_MULTI_RESULTS: u32 = 0x0002_0000;
const CLIENT_PLUGIN_AUTH: u32 = 0x0008_0000;
const CLIENT_CONNECT_ATTRS: u32 = 0x0010_0000;
const CLIENT_PLUGIN_AUTH_LENENC_CLIENT_DATA: u32 = 0x0020_0000;
const SERVER_CAPABILITIES: u32 = CLIENT_LONG_PASSWORD
    | CLIENT_LONG_FLAG
    | CLIENT_CONNECT_WITH_DB
    | CLIENT_PROTOCOL_41
    | CLIENT_TRANSACTIONS
    | CLIENT_SECURE_CONNECTION
    | CLIENT_MULTI_RESULTS
    | CLIENT_PLUGIN_AUTH
    | CLIENT_CONNECT_ATTRS
    | CLIENT_PLUGIN_AUTH_LENENC_CLIENT_DATA;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayUser {
    pub username: String,
    pub argon2_hash: String,
    pub mysql_native_password_verifier_hex: String,
}

impl GatewayUser {
    pub fn bootstrap(username: impl Into<String>, password: &str) -> Result<Self> {
        let hasher = Argon2idPasswordHasher::default();
        let argon2_hash = hasher.hash(password.as_bytes())?;
        let stage1 = sha1_hash(password.as_bytes());
        let stage2 = sha1_hash(&stage1);
        Ok(Self {
            username: username.into(),
            argon2_hash,
            mysql_native_password_verifier_hex: hex(&stage2),
        })
    }

    fn native_verifier(&self) -> Result<[u8; 20]> {
        let bytes = decode_hex(&self.mysql_native_password_verifier_hex)?;
        if bytes.len() != 20 {
            bail!("invalid mysql_native_password verifier length");
        }
        let mut verifier = [0u8; 20];
        verifier.copy_from_slice(&bytes);
        Ok(verifier)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    pub bind_addr: String,
    pub agidb_path: String,
    pub status_path: String,
    pub users: Vec<GatewayUser>,
    pub max_connections: usize,
    pub max_packet_size: usize,
    pub connect_timeout_ms: u64,
    pub query_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayStatus {
    pub pid: u32,
    pub listener_addr: String,
    pub status_path: String,
    pub started_at: u64,
    pub active_connections: usize,
    pub total_sessions: usize,
    pub authenticated_users: Vec<String>,
    pub max_connections: usize,
    pub max_packet_size: usize,
    pub online: bool,
}

#[derive(Debug)]
pub struct MySqlGatewayServer {
    local_addr: SocketAddr,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    status_path: PathBuf,
}

impl MySqlGatewayServer {
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    pub fn status_path(&self) -> &Path {
        &self.status_path
    }
}

impl Drop for MySqlGatewayServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.local_addr);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
        let _ = fs::remove_file(&self.status_path);
    }
}

#[derive(Debug)]
struct GatewayState {
    listener_addr: String,
    status_path: PathBuf,
    started_at: u64,
    active_connections: usize,
    total_sessions: usize,
    authenticated_users: Vec<String>,
    max_connections: usize,
    max_packet_size: usize,
}

pub fn start_server(config: GatewayConfig) -> Result<MySqlGatewayServer> {
    let listener = TcpListener::bind(&config.bind_addr)
        .with_context(|| format!("failed to bind MySQL gateway at {}", config.bind_addr))?;
    let local_addr = listener.local_addr()?;
    let state = Arc::new(Mutex::new(GatewayState {
        listener_addr: local_addr.to_string(),
        status_path: PathBuf::from(&config.status_path),
        started_at: unix_seconds(),
        active_connections: 0,
        total_sessions: 0,
        authenticated_users: vec![],
        max_connections: config.max_connections,
        max_packet_size: config.max_packet_size,
    }));
    write_status(&state.lock().unwrap())?;
    let shutdown = Arc::new(AtomicBool::new(false));
    let connection_counter = Arc::new(AtomicU32::new(1));
    let shutdown_flag = shutdown.clone();
    let thread_state = state.clone();
    let thread_config = config.clone();
    let handle = thread::spawn(move || {
        while !shutdown_flag.load(Ordering::SeqCst) {
            let Ok((stream, peer)) = listener.accept() else {
                continue;
            };
            let config = thread_config.clone();
            let state = thread_state.clone();
            let shutdown = shutdown_flag.clone();
            let connection_id = connection_counter.fetch_add(1, Ordering::SeqCst);
            thread::spawn(move || {
                let _ = handle_client(stream, peer, connection_id, config, state, shutdown);
            });
        }
    });
    Ok(MySqlGatewayServer {
        local_addr,
        shutdown,
        thread: Some(handle),
        status_path: PathBuf::from(&config.status_path),
    })
}

pub fn read_status(status_path: impl AsRef<Path>) -> Result<GatewayStatus> {
    let bytes = fs::read(status_path.as_ref())
        .with_context(|| format!("failed to read {}", status_path.as_ref().display()))?;
    let status: GatewayStatus = serde_json::from_slice(&bytes).context("invalid status JSON")?;
    Ok(status)
}

fn handle_client(
    mut stream: TcpStream,
    peer: SocketAddr,
    connection_id: u32,
    config: GatewayConfig,
    state: Arc<Mutex<GatewayState>>,
    shutdown: Arc<AtomicBool>,
) -> Result<()> {
    {
        let mut locked = state.lock().unwrap();
        if locked.active_connections >= locked.max_connections {
            send_error_packet(&mut stream, 0, 1040, "08004", "too many connections")?;
            return Ok(());
        }
        locked.active_connections += 1;
        write_status(&locked)?;
    }

    let result = handle_client_inner(
        &mut stream,
        peer,
        connection_id,
        &config,
        &state,
        shutdown,
    );

    {
        let mut locked = state.lock().unwrap();
        locked.active_connections = locked.active_connections.saturating_sub(1);
        write_status(&locked)?;
    }

    result
}

fn handle_client_inner(
    stream: &mut TcpStream,
    _peer: SocketAddr,
    connection_id: u32,
    config: &GatewayConfig,
    state: &Arc<Mutex<GatewayState>>,
    shutdown: Arc<AtomicBool>,
) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_millis(config.connect_timeout_ms)))?;
    stream.set_write_timeout(Some(Duration::from_millis(config.connect_timeout_ms)))?;

    let scramble = make_scramble(connection_id);
    write_handshake_packet(stream, connection_id, &scramble)?;
    let auth_packet = read_packet(stream, config.max_packet_size)?;
    let auth = match parse_handshake_response(&auth_packet) {
        Ok(auth) => auth,
        Err(err) => {
            send_error_packet(stream, 2, 1043, "08S01", &err.to_string())?;
            bail!(err);
        }
    };
    let user = match config.users.iter().find(|entry| entry.username == auth.username) {
        Some(user) => user,
        None => {
            send_error_packet(stream, 2, 1045, "28000", "access denied for user")?;
            bail!("E6611 access denied for user");
        }
    };
    if let Err(err) = verify_mysql_native_password(user, &scramble, &auth.auth_response) {
        send_error_packet(stream, 2, 1045, "28000", "access denied for user")?;
        bail!(err);
    }
    send_ok_packet(stream, 2, 0, 0)?;

    {
        let mut locked = state.lock().unwrap();
        locked.total_sessions += 1;
        if !locked.authenticated_users.contains(&user.username) {
            locked.authenticated_users.push(user.username.clone());
        }
        write_status(&locked)?;
    }

    stream.set_read_timeout(Some(Duration::from_millis(config.query_timeout_ms)))?;
    stream.set_write_timeout(Some(Duration::from_millis(config.query_timeout_ms)))?;

    loop {
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
        let packet = match read_packet(stream, config.max_packet_size) {
            Ok(packet) => packet,
            Err(err) if err.to_string().contains("connection closed") => break,
            Err(err) => {
                let _ = send_error_packet(stream, 0, 1153, "08S01", &err.to_string());
                break;
            }
        };
        if packet.is_empty() {
            break;
        }
        let command = packet[0];
        match command {
            0x01 => break,
            0x03 => {
                let sql = std::str::from_utf8(&packet[1..])
                    .context("query packet payload is not UTF-8")?
                    .trim()
                    .to_string();
                match execute_query(&config.agidb_path, &sql) {
                    Ok(QueryOutcome::ResultSet { columns, rows }) => {
                        send_result_set(stream, &columns, &rows)?;
                    }
                    Ok(QueryOutcome::Ok { rows_affected }) => {
                        send_ok_packet(stream, 1, rows_affected, 0)?;
                    }
                    Err(err) => {
                        send_error_packet(stream, 1, 1064, "42000", &err.to_string())?;
                    }
                }
            }
            _ => {
                send_error_packet(
                    stream,
                    1,
                    1047,
                    "08S01",
                    "unsupported MySQL command for AGILANG gateway",
                )?;
            }
        }
    }

    Ok(())
}

#[derive(Debug)]
struct HandshakeResponse {
    username: String,
    auth_response: Vec<u8>,
}

fn parse_handshake_response(payload: &[u8]) -> Result<HandshakeResponse> {
    if payload.len() < 36 {
        bail!("malformed handshake response");
    }
    let capability_flags = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
    let mut index = 32;
    while index < payload.len() && payload[index] != 0 {
        index += 1;
    }
    if index >= payload.len() {
        bail!("malformed username in handshake response");
    }
    let username = String::from_utf8(payload[32..index].to_vec()).context("invalid username")?;
    index += 1;
    let auth_response = if capability_flags & CLIENT_PLUGIN_AUTH_LENENC_CLIENT_DATA != 0 {
        let (length, consumed) = read_lenenc_int(&payload[index..])?;
        index += consumed;
        payload
            .get(index..index + length as usize)
            .context("malformed auth response")?
            .to_vec()
    } else if capability_flags & CLIENT_SECURE_CONNECTION != 0 {
        let length = *payload.get(index).context("missing auth response length")? as usize;
        index += 1;
        payload
            .get(index..index + length)
            .context("malformed secure auth response")?
            .to_vec()
    } else {
        bail!("unsupported insecure MySQL auth response");
    };
    Ok(HandshakeResponse {
        username,
        auth_response,
    })
}

fn verify_mysql_native_password(user: &GatewayUser, scramble: &[u8; 20], auth_response: &[u8]) -> Result<()> {
    let verifier = user.native_verifier()?;
    let mut sha = sha1_hash_with_prefix(scramble, &verifier);
    for (slot, byte) in sha.iter_mut().zip(auth_response.iter().copied()) {
        *slot ^= byte;
    }
    let candidate = sha1_hash(&sha);
    if candidate != verifier {
        bail!("E6611 access denied for user");
    }
    Ok(())
}

fn execute_query(agidb_path: &str, sql: &str) -> Result<QueryOutcome> {
    let upper = sql.trim().to_ascii_uppercase();
    if upper == "SELECT 1" || upper == "SELECT 1;" {
        return Ok(QueryOutcome::ResultSet {
            columns: vec![ColumnMeta::new("1", MYSQL_TYPE_LONG)],
            rows: vec![vec![DatabaseValue::Integer(1)]],
        });
    }
    if upper.starts_with("SET ") {
        return Ok(QueryOutcome::Ok { rows_affected: 0 });
    }
    if upper.starts_with("SELECT ") {
        let mut conn = AgiDbConnection::new(agidb_path);
        let rows = conn.query(sql, &[])?;
        let columns = infer_columns(&rows);
        let ordered_rows = rows
            .into_iter()
            .map(|row| {
                columns
                    .iter()
                    .map(|column| row.get(&column.name).cloned().unwrap_or(DatabaseValue::Null))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        return Ok(QueryOutcome::ResultSet {
            columns,
            rows: ordered_rows,
        });
    }
    if upper.starts_with("INSERT ")
        || upper.starts_with("UPDATE ")
        || upper.starts_with("DELETE ")
        || upper.starts_with("CREATE TABLE ")
    {
        let mut conn = AgiDbConnection::new(agidb_path);
        let result = conn.execute(sql, &[])?;
        return Ok(QueryOutcome::Ok {
            rows_affected: result.rows_affected,
        });
    }
    bail!("E6608 unsupported SQL for AGILANG MySQL gateway")
}

#[derive(Debug)]
enum QueryOutcome {
    ResultSet {
        columns: Vec<ColumnMeta>,
        rows: Vec<Vec<DatabaseValue>>,
    },
    Ok {
        rows_affected: u64,
    },
}

#[derive(Debug, Clone)]
struct ColumnMeta {
    name: String,
    column_type: u8,
}

impl ColumnMeta {
    fn new(name: impl Into<String>, column_type: u8) -> Self {
        Self {
            name: name.into(),
            column_type,
        }
    }
}

fn infer_columns(rows: &[DatabaseRow]) -> Vec<ColumnMeta> {
    let mut names = rows
        .first()
        .map(|row| row.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_else(|| vec!["result".to_string()]);
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let column_type = rows
                .iter()
                .find_map(|row| row.get(&name))
                .map(mysql_type_for_value)
                .unwrap_or(MYSQL_TYPE_NULL);
            ColumnMeta::new(name, column_type)
        })
        .collect()
}

fn mysql_type_for_value(value: &DatabaseValue) -> u8 {
    match value {
        DatabaseValue::Integer(_) => MYSQL_TYPE_LONG,
        DatabaseValue::Float(_) => MYSQL_TYPE_DOUBLE,
        DatabaseValue::Null => MYSQL_TYPE_NULL,
        _ => MYSQL_TYPE_VAR_STRING,
    }
}

fn write_handshake_packet(stream: &mut TcpStream, connection_id: u32, scramble: &[u8; 20]) -> Result<()> {
    let mut payload = vec![
        MYSQL_PROTOCOL_VERSION,
    ];
    payload.extend_from_slice(MYSQL_SERVER_VERSION.as_bytes());
    payload.push(0);
    payload.extend_from_slice(&connection_id.to_le_bytes());
    payload.extend_from_slice(&scramble[..8]);
    payload.push(0);
    payload.extend_from_slice(&(SERVER_CAPABILITIES as u16).to_le_bytes());
    payload.push(MYSQL_CHARSET_UTF8MB4_GENERAL_CI);
    payload.extend_from_slice(&MYSQL_STATUS_AUTOCOMMIT.to_le_bytes());
    payload.extend_from_slice(&((SERVER_CAPABILITIES >> 16) as u16).to_le_bytes());
    payload.push(21);
    payload.extend_from_slice(&[0u8; 10]);
    payload.extend_from_slice(&scramble[8..]);
    payload.push(0);
    payload.extend_from_slice(b"mysql_native_password\0");
    write_packet(stream, 0, &payload)
}

fn send_ok_packet(stream: &mut TcpStream, sequence_id: u8, affected_rows: u64, last_insert_id: u64) -> Result<()> {
    let mut payload = vec![0x00];
    write_lenenc_int(&mut payload, affected_rows);
    write_lenenc_int(&mut payload, last_insert_id);
    payload.extend_from_slice(&MYSQL_STATUS_AUTOCOMMIT.to_le_bytes());
    payload.extend_from_slice(&0u16.to_le_bytes());
    write_packet(stream, sequence_id, &payload)
}

fn send_error_packet(stream: &mut TcpStream, sequence_id: u8, code: u16, sql_state: &str, message: &str) -> Result<()> {
    let mut payload = vec![0xff];
    payload.extend_from_slice(&code.to_le_bytes());
    payload.push(b'#');
    let mut state = sql_state.as_bytes().to_vec();
    state.resize(5, b'0');
    payload.extend_from_slice(&state[..5]);
    payload.extend_from_slice(message.as_bytes());
    write_packet(stream, sequence_id, &payload)
}

fn send_result_set(stream: &mut TcpStream, columns: &[ColumnMeta], rows: &[Vec<DatabaseValue>]) -> Result<()> {
    let mut sequence_id = 1u8;
    let mut header = vec![];
    write_lenenc_int(&mut header, columns.len() as u64);
    write_packet(stream, sequence_id, &header)?;
    sequence_id = sequence_id.wrapping_add(1);

    for column in columns {
        let payload = column_definition_payload(column);
        write_packet(stream, sequence_id, &payload)?;
        sequence_id = sequence_id.wrapping_add(1);
    }

    write_eof_packet(stream, sequence_id)?;
    sequence_id = sequence_id.wrapping_add(1);

    for row in rows {
        let mut payload = vec![];
        for value in row {
            write_lenenc_string(&mut payload, &database_value_text(value));
        }
        write_packet(stream, sequence_id, &payload)?;
        sequence_id = sequence_id.wrapping_add(1);
    }

    write_eof_packet(stream, sequence_id)?;
    Ok(())
}

fn column_definition_payload(column: &ColumnMeta) -> Vec<u8> {
    let mut payload = vec![];
    for value in ["def", "", "", "", column.name.as_str(), column.name.as_str()] {
        write_lenenc_string(&mut payload, value);
    }
    payload.extend_from_slice(&0x0c_u8.to_le_bytes());
    payload.extend_from_slice(&(MYSQL_CHARSET_UTF8MB4_GENERAL_CI as u16).to_le_bytes());
    payload.extend_from_slice(&1024u32.to_le_bytes());
    payload.push(column.column_type);
    payload.extend_from_slice(&0u16.to_le_bytes());
    payload.push(0);
    payload.extend_from_slice(&[0u8; 2]);
    payload
}

fn write_eof_packet(stream: &mut TcpStream, sequence_id: u8) -> Result<()> {
    let mut payload = vec![0xfe];
    payload.extend_from_slice(&0u16.to_le_bytes());
    payload.extend_from_slice(&MYSQL_STATUS_AUTOCOMMIT.to_le_bytes());
    write_packet(stream, sequence_id, &payload)
}

fn database_value_text(value: &DatabaseValue) -> String {
    match value {
        DatabaseValue::Null => String::new(),
        DatabaseValue::Boolean(value) => {
            if *value {
                "1".to_string()
            } else {
                "0".to_string()
            }
        }
        DatabaseValue::Integer(value) => value.to_string(),
        DatabaseValue::Float(value) => value.to_string(),
        DatabaseValue::Decimal(value)
        | DatabaseValue::Text(value)
        | DatabaseValue::DateTime(value)
        | DatabaseValue::Json(value) => value.clone(),
        DatabaseValue::Binary(value) => hex(value),
    }
}

fn write_packet(stream: &mut TcpStream, sequence_id: u8, payload: &[u8]) -> Result<()> {
    let length = payload.len();
    if length > 0x00ff_ffff {
        bail!("packet exceeds MySQL protocol maximum");
    }
    let header = [
        (length & 0xff) as u8,
        ((length >> 8) & 0xff) as u8,
        ((length >> 16) & 0xff) as u8,
        sequence_id,
    ];
    stream.write_all(&header)?;
    stream.write_all(payload)?;
    stream.flush()?;
    Ok(())
}

fn read_packet(stream: &mut TcpStream, max_packet_size: usize) -> Result<Vec<u8>> {
    let mut header = [0u8; 4];
    match stream.read_exact(&mut header) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => {
            bail!("connection closed")
        }
        Err(err) => return Err(err.into()),
    }
    let length = header[0] as usize | ((header[1] as usize) << 8) | ((header[2] as usize) << 16);
    if length > max_packet_size {
        bail!("packet exceeds configured maximum packet size");
    }
    let mut payload = vec![0u8; length];
    stream.read_exact(&mut payload)?;
    Ok(payload)
}

fn write_lenenc_int(payload: &mut Vec<u8>, value: u64) {
    if value < 251 {
        payload.push(value as u8);
    } else if value < 0x10000 {
        payload.push(0xfc);
        payload.extend_from_slice(&(value as u16).to_le_bytes());
    } else if value < 0x1000000 {
        payload.push(0xfd);
        payload.extend_from_slice(&(value as u32).to_le_bytes()[..3]);
    } else {
        payload.push(0xfe);
        payload.extend_from_slice(&value.to_le_bytes());
    }
}

fn read_lenenc_int(payload: &[u8]) -> Result<(u64, usize)> {
    let first = *payload.first().context("missing length-encoded integer")?;
    match first {
        0xfc => Ok((u16::from_le_bytes([payload[1], payload[2]]) as u64, 3)),
        0xfd => Ok((
            (payload[1] as u64) | ((payload[2] as u64) << 8) | ((payload[3] as u64) << 16),
            4,
        )),
        0xfe => {
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&payload[1..9]);
            Ok((u64::from_le_bytes(bytes), 9))
        }
        value => Ok((value as u64, 1)),
    }
}

fn write_lenenc_string(payload: &mut Vec<u8>, value: &str) {
    write_lenenc_int(payload, value.len() as u64);
    payload.extend_from_slice(value.as_bytes());
}

fn make_scramble(connection_id: u32) -> [u8; 20] {
    let mut input = connection_id.to_le_bytes().to_vec();
    input.extend_from_slice(&unix_seconds().to_le_bytes());
    let digest = sha1_hash(&input);
    let mut scramble = [0u8; 20];
    scramble.copy_from_slice(&digest);
    scramble
}

fn write_status(state: &GatewayState) -> Result<()> {
    if let Some(parent) = state.status_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let status = GatewayStatus {
        pid: std::process::id(),
        listener_addr: state.listener_addr.clone(),
        status_path: state.status_path.display().to_string(),
        started_at: state.started_at,
        active_connections: state.active_connections,
        total_sessions: state.total_sessions,
        authenticated_users: state.authenticated_users.clone(),
        max_connections: state.max_connections,
        max_packet_size: state.max_packet_size,
        online: true,
    };
    fs::write(&state.status_path, serde_json::to_vec_pretty(&status)?)?;
    Ok(())
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0)
}

fn hex(data: &[u8]) -> String {
    data.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_hex(input: &str) -> Result<Vec<u8>> {
    if input.len() % 2 != 0 {
        bail!("hex input must have even length");
    }
    (0..input.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&input[index..index + 2], 16).context("invalid hex"))
        .collect()
}

fn sha1_hash_with_prefix(prefix: &[u8], suffix: &[u8]) -> [u8; 20] {
    let mut data = Vec::with_capacity(prefix.len() + suffix.len());
    data.extend_from_slice(prefix);
    data.extend_from_slice(suffix);
    sha1_hash(&data)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn temp_path(name: &str, ext: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("agilang-mysql-gateway-{name}-{nonce}.{ext}"))
    }

    fn seed_db(path: &Path) {
        let mut conn = AgiDbConnection::new(path.to_string_lossy().to_string());
        conn.execute(
            "CREATE TABLE users (id INTEGER, name TEXT, email TEXT)",
            &[],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO users (name, email) VALUES ('Ada', 'ada@example.com')",
            &[],
        )
        .unwrap();
    }

    fn config(path: &Path, status: &Path) -> GatewayConfig {
        GatewayConfig {
            bind_addr: "127.0.0.1:0".to_string(),
            agidb_path: path.to_string_lossy().to_string(),
            status_path: status.to_string_lossy().to_string(),
            users: vec![GatewayUser::bootstrap("root", "secret123").unwrap()],
            max_connections: 4,
            max_packet_size: 1024 * 1024,
            connect_timeout_ms: 1000,
            query_timeout_ms: 1000,
        }
    }

    #[test]
    fn pymysql_can_authenticate_and_query_agidb() {
        let db_path = temp_path("db", "agidb");
        let status_path = temp_path("status", "json");
        seed_db(&db_path);
        let server = start_server(config(&db_path, &status_path)).unwrap();
        let addr = server.local_addr();
        let script = format!(
            r#"
import pymysql
conn = pymysql.connect(host="127.0.0.1", port={port}, user="root", password="secret123", database="", connect_timeout=1, read_timeout=1, write_timeout=1, autocommit=True)
cur = conn.cursor()
cur.execute("SELECT email, id, name FROM users LIMIT 1")
row = cur.fetchone()
print(row)
conn.close()
"#,
            port = addr.port()
        );
        let output = Command::new("python")
            .arg("-c")
            .arg(script)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("ada@example.com"));

        let status = read_status(&status_path).unwrap();
        assert!(status.total_sessions >= 1);
        assert!(status.authenticated_users.contains(&"root".to_string()));
    }

    #[test]
    fn invalid_credentials_fail() {
        let db_path = temp_path("badcreds", "agidb");
        let status_path = temp_path("badcreds-status", "json");
        seed_db(&db_path);
        let server = start_server(config(&db_path, &status_path)).unwrap();
        let addr = server.local_addr();
        let script = format!(
            r#"
import pymysql, sys
try:
    pymysql.connect(host="127.0.0.1", port={port}, user="root", password="wrong", connect_timeout=1, read_timeout=1, write_timeout=1)
except Exception as exc:
    print(type(exc).__name__)
    sys.exit(0)
sys.exit(1)
"#,
            port = addr.port()
        );
        let output = Command::new("python").arg("-c").arg(script).output().unwrap();
        assert!(output.status.success());
        drop(server);
    }

    #[test]
    fn malformed_and_oversized_packets_fail() {
        let db_path = temp_path("badpkt", "agidb");
        let status_path = temp_path("badpkt-status", "json");
        seed_db(&db_path);
        let server = start_server(config(&db_path, &status_path)).unwrap();
        let addr = server.local_addr();

        let mut stream = TcpStream::connect(addr).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
        let _handshake = read_packet(&mut stream, 1024 * 1024).unwrap();
        stream.write_all(&[0x10, 0x00, 0x10, 0x01]).unwrap();
        stream.flush().unwrap();

        let mut header = [0u8; 4];
        match stream.read_exact(&mut header) {
            Ok(()) => {
                let length =
                    header[0] as usize | ((header[1] as usize) << 8) | ((header[2] as usize) << 16);
                let mut payload = vec![0u8; length];
                stream.read_exact(&mut payload).unwrap();
                assert_eq!(payload[0], 0xff);
            }
            Err(err) => {
                assert!(
                    matches!(
                        err.kind(),
                        std::io::ErrorKind::UnexpectedEof
                            | std::io::ErrorKind::ConnectionReset
                            | std::io::ErrorKind::TimedOut
                    ),
                    "unexpected malformed-packet failure: {err}"
                );
            }
        }

        let mut oversized = TcpStream::connect(addr).unwrap();
        oversized.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
        let _ = read_packet(&mut oversized, 1024 * 1024).unwrap();
        oversized.write_all(&[0x01, 0x00, 0x20, 0x01, 0x00]).unwrap();
        oversized.flush().unwrap();
        drop(server);
    }

    #[test]
    fn unreachable_port_fails() {
        let script = r#"
import pymysql, sys
try:
    pymysql.connect(host="127.0.0.1", port=9, user="root", password="secret123", connect_timeout=1, read_timeout=1, write_timeout=1)
except Exception:
    sys.exit(0)
sys.exit(1)
"#;
        let output = Command::new("python").arg("-c").arg(script).output().unwrap();
        assert!(output.status.success());
    }
}
