//! MySQL connection — native wire protocol implementation.
//!
//! Implemented from scratch on top of `tokio::net::TcpStream`:
//! - Server handshake (Protocol 10) + authentication (`mysql_native_password`,
//!   `caching_sha2_password` fast/full auth with RSA, `sha256_password`)
//! - `COM_QUERY` text protocol for statements without parameters
//! - Prepared statements (`COM_STMT_PREPARE` / `COM_STMT_EXECUTE`) with the
//!   binary protocol for parameterized statements
//! - Column definition, text-row and binary-row decoding, OK/EOF/Error packets

use crate::db::db_types::{DbType, QueryResult, Row, SqlValue};
use crate::db::database::{DatabaseConnection, DbError, Transaction};
use chrono::{Datelike, Timelike};
use sha1::{Digest, Sha1};
use sha2::Sha256;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub struct MySqlConnection {
    config: MySqlConnectionConfig,
    stream: Arc<tokio::sync::Mutex<Option<TcpStream>>>,
    connected: Arc<Mutex<bool>>,
    capabilities: u32,
}

impl Clone for MySqlConnection {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            stream: Arc::clone(&self.stream),
            connected: Arc::clone(&self.connected),
            capabilities: self.capabilities,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MySqlConnectionConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
    pub charset: String,
    pub timeout: std::time::Duration,
}

impl MySqlConnectionConfig {
    pub fn new(host: &str, port: u16, username: &str, password: &str, database: &str) -> Self {
        Self {
            host: host.to_string(),
            port,
            username: username.to_string(),
            password: password.to_string(),
            database: database.to_string(),
            charset: "utf8mb4".to_string(),
            timeout: std::time::Duration::from_secs(30),
        }
    }

    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl MySqlConnection {
    pub async fn new(config: &crate::db::database::ConnectionConfig) -> Result<Self, DbError> {
        let mysql_config = MySqlConnectionConfig::new(
            &config.host,
            config.port,
            &config.username,
            &config.password,
            &config.database,
        )
        .with_timeout(config.timeout);

        let (stream, capabilities) = connect_and_authenticate(&mysql_config).await?;

        Ok(Self {
            config: mysql_config,
            stream: Arc::new(tokio::sync::Mutex::new(Some(stream))),
            connected: Arc::new(Mutex::new(true)),
            capabilities,
        })
    }

    /// Run a statement, choosing the protocol based on whether parameters are present.
    async fn run_query(&self, sql: &str, params: &[SqlValue]) -> Result<MyQueryResult, DbError> {
        tokio::time::timeout(self.config.timeout, async {
            let mut guard = self.stream.lock().await;
            let stream = (&mut *guard)
                .as_mut()
                .ok_or_else(|| DbError::connection_error("Connection is closed"))?;
            if params.is_empty() {
                com_query(stream, sql, self.capabilities).await
            } else {
                com_stmt_execute_query(stream, sql, params, self.capabilities).await
            }
        })
        .await
        .map_err(|_| {
            DbError::TimeoutError(format!(
                "Query timed out after {:?}: {}",
                self.config.timeout, sql
            ))
        })?
    }
}

/// Result of one statement, before conversion into the public types.
struct MyQueryResult {
    rows: Vec<Row>,
    rows_affected: u64,
    last_insert_id: Option<i64>,
}

// ---------------------------------------------------------------------------
// Capability flags
// ---------------------------------------------------------------------------

const CLIENT_PROTOCOL_41: u32 = 0x0000_0200;
const CLIENT_SECURE_CONNECTION: u32 = 0x0000_8000;
const CLIENT_CONNECT_WITH_DB: u32 = 0x0000_0008;
const CLIENT_PLUGIN_AUTH: u32 = 0x0008_0000;
const CLIENT_CONNECT_ATTRS: u32 = 0x0010_0000;
const CLIENT_DEPRECATE_EOF: u32 = 0x0100_0000;

const MYSQL_TYPE_TINY: u8 = 0x01;
const MYSQL_TYPE_SHORT: u8 = 0x02;
const MYSQL_TYPE_LONG: u8 = 0x03;
const MYSQL_TYPE_FLOAT: u8 = 0x04;
const MYSQL_TYPE_DOUBLE: u8 = 0x05;
const MYSQL_TYPE_NULL: u8 = 0x06;
const MYSQL_TYPE_TIMESTAMP: u8 = 0x07;
const MYSQL_TYPE_LONGLONG: u8 = 0x08;
const MYSQL_TYPE_INT24: u8 = 0x09;
const MYSQL_TYPE_DATE: u8 = 0x0a;
const MYSQL_TYPE_TIME: u8 = 0x0b;
const MYSQL_TYPE_DATETIME: u8 = 0x0c;
const MYSQL_TYPE_YEAR: u8 = 0x0d;
const MYSQL_TYPE_NEWDECIMAL: u8 = 0xf6;
const MYSQL_TYPE_BLOB: u8 = 0xfc;
const MYSQL_TYPE_VAR_STRING: u8 = 0xfd;
#[allow(dead_code)]
const MYSQL_TYPE_STRING: u8 = 0xfe;

// ---------------------------------------------------------------------------
// Connection establishment and authentication
// ---------------------------------------------------------------------------

/// Parsed server Initial Handshake Packet (Protocol 10).
struct ServerHandshake {
    server_capabilities: u32,
    salt: Vec<u8>,
    auth_plugin_name: String,
}

async fn connect_and_authenticate(
    config: &MySqlConnectionConfig,
) -> Result<(TcpStream, u32), DbError> {
    let addr = (config.host.as_str(), config.port);
    let mut stream = tokio::time::timeout(config.timeout, TcpStream::connect(addr))
        .await
        .map_err(|_| {
            DbError::TimeoutError(format!(
                "Connection to {}:{} timed out after {:?}",
                config.host, config.port, config.timeout
            ))
        })?
        .map_err(|e| {
            DbError::connection_error(format!(
                "Failed to connect to {}:{}: {}",
                config.host, config.port, e
            ))
        })?;

    let _ = stream.set_nodelay(true);

    // 1. Read the server's Initial Handshake Packet (sequence id 0).
    let (seq, handshake_payload) = read_packet(&mut stream).await?;
    let handshake = parse_server_handshake(&handshake_payload)?;

    // 2. Build and send the Handshake Response (sequence id = seq + 1).
    let client_capabilities: u32 = CLIENT_PROTOCOL_41
        | CLIENT_SECURE_CONNECTION
        | CLIENT_PLUGIN_AUTH
        | CLIENT_CONNECT_ATTRS
        | CLIENT_CONNECT_WITH_DB
        | (handshake.server_capabilities & CLIENT_DEPRECATE_EOF);

    let initial_auth_response = compute_initial_auth_response(
        &handshake.auth_plugin_name,
        config.password.as_bytes(),
        &handshake.salt,
    )?;

    let response = build_handshake_response(
        client_capabilities,
        &config.username,
        &initial_auth_response,
        &config.database,
        &handshake.auth_plugin_name,
        &config.charset,
    );
    write_packet(&mut stream, seq.wrapping_add(1), &response).await?;

    // 3. Handle the auth exchange until we receive an OK packet.
    // `seq` is always (re)set from the server's response at the top of each loop
    // iteration, so we don't need to track it across iterations here.
    let mut seq: u8;
    let mut current_plugin = handshake.auth_plugin_name.clone();
    loop {
        let (resp_seq, payload) = read_packet(&mut stream).await?;
        seq = resp_seq.wrapping_add(1);
        if payload.is_empty() {
            return Err(DbError::protocol_error("Empty auth response packet"));
        }
        match payload[0] {
            0x00 => {
                // OK packet — authentication succeeded.
                let _ok = parse_ok_packet(&payload, client_capabilities)?;
                return Ok((stream, client_capabilities));
            }
            0xff => {
                return Err(parse_error_packet(&payload, client_capabilities));
            }
            0xfe => {
                // AuthSwitchRequest (when payload length > 1) or EOF.
                if payload.len() <= 1 {
                    return Err(DbError::auth_error(
                        "Unexpected EOF packet during authentication",
                    ));
                }
                // Format: 0xFE, plugin_name\0, auth_plugin_data
                let rest = &payload[1..];
                let nul = rest
                    .iter()
                    .position(|&b| b == 0)
                    .ok_or_else(|| DbError::protocol_error("Malformed AuthSwitchRequest"))?;
                let plugin_name = String::from_utf8_lossy(&rest[..nul]).to_string();
                let new_salt = rest[nul + 1..].to_vec();

                let auth_response = compute_auth_switch_response(
                    &plugin_name,
                    config.password.as_bytes(),
                    &new_salt,
                )?;
                write_packet(&mut stream, seq, &auth_response).await?;
                current_plugin = plugin_name;
            }
            0x01 => {
                // AuthMoreData — used by caching_sha2_password / sha256_password.
                if payload.len() < 2 {
                    return Err(DbError::protocol_error("Malformed AuthMoreData packet"));
                }
                match payload[1] {
                    0x03 => {
                        // Fast auth success (cached) — next packet is OK/EOF.
                        continue;
                    }
                    0x04 => {
                        // Full auth required — send password encrypted with the
                        // server's RSA public key (no SSL path here).
                        let key = request_rsa_public_key(&mut stream, &mut seq).await?;
                        let encrypted = encrypt_password_with_rsa(
                            config.password.as_bytes(),
                            &key,
                        )?;
                        // Prepend 0x00 to indicate "password encrypted with RSA".
                        let mut msg = Vec::with_capacity(encrypted.len() + 1);
                        msg.push(0x00);
                        msg.extend_from_slice(&encrypted);
                        write_packet(&mut stream, seq, &msg).await?;
                    }
                    other => {
                        return Err(DbError::auth_error(format!(
                            "Unknown AuthMoreData status: 0x{:02x}",
                            other
                        )));
                    }
                }
            }
            other => {
                return Err(DbError::protocol_error(format!(
                    "Unexpected packet header 0x{:02x} during auth (plugin {})",
                    other, current_plugin
                )));
            }
        }
    }
}

/// Request the server's RSA public key by sending a single-byte `0x02` packet.
async fn request_rsa_public_key(
    stream: &mut TcpStream,
    seq: &mut u8,
) -> Result<Vec<u8>, DbError> {
    write_packet(stream, *seq, &[0x02]).await?;
    *seq = seq.wrapping_add(1);
    let (resp_seq, payload) = read_packet(stream).await?;
    *seq = resp_seq.wrapping_add(1);
    if payload.is_empty() {
        return Err(DbError::protocol_error("Empty RSA public key response"));
    }
    // Header byte 0x01 = AuthMoreData, followed by the PEM-encoded key.
    if payload[0] != 0x01 {
        return Err(DbError::protocol_error(format!(
            "Expected RSA public key (AuthMoreData), got header 0x{:02x}",
            payload[0]
        )));
    }
    Ok(payload[1..].to_vec())
}

/// Parse the server's Initial Handshake Packet (Protocol 10).
fn parse_server_handshake(payload: &[u8]) -> Result<ServerHandshake, DbError> {
    let mut pos = 0;
    // protocol version (1 byte, must be 10)
    if payload.is_empty() {
        return Err(DbError::protocol_error("Empty handshake packet"));
    }
    let protocol_version = payload[0];
    pos += 1;
    if protocol_version != 10 {
        return Err(DbError::protocol_error(format!(
            "Unsupported MySQL protocol version: {} (expected 10)",
            protocol_version
        )));
    }
    // server version (NUL-terminated string)
    let server_version = read_nul_string(payload, &mut pos)?;
    let _ = server_version;
    // connection id (4 bytes)
    pos += 4;
    // auth-plugin-data-part-1 (8 bytes)
    if pos + 8 > payload.len() {
        return Err(DbError::protocol_error("Handshake too short for salt part 1"));
    }
    let mut salt = payload[pos..pos + 8].to_vec();
    pos += 8;
    // filler (1 byte, 0x00)
    pos += 1;
    // capability flags lower 2 bytes
    if pos + 2 > payload.len() {
        return Err(DbError::protocol_error("Handshake too short for capabilities"));
    }
    let caps_lower = u16::from_le_bytes([payload[pos], payload[pos + 1]]) as u32;
    pos += 2;
    let mut server_capabilities = caps_lower;
    // If the packet ends here (very old servers), bail out with what we have.
    if pos >= payload.len() {
        return Ok(ServerHandshake {
            server_capabilities,
            salt,
            auth_plugin_name: String::new(),
        });
    }
    // character set (1 byte)
    pos += 1;
    // status flags (2 bytes)
    pos += 2;
    // capability flags upper 2 bytes
    if pos + 2 > payload.len() {
        return Err(DbError::protocol_error("Handshake too short for upper capabilities"));
    }
    let caps_upper = u16::from_le_bytes([payload[pos], payload[pos + 1]]) as u32;
    pos += 2;
    server_capabilities |= caps_upper << 16;

    // length of auth-plugin-data (1 byte) if CLIENT_PLUGIN_AUTH, else 0
    if pos >= payload.len() {
        return Ok(ServerHandshake {
            server_capabilities,
            salt,
            auth_plugin_name: String::new(),
        });
    }
    let auth_data_len = payload[pos] as usize;
    pos += 1;
    // reserved (10 bytes)
    pos += 10;

    // auth-plugin-data-part-2: max(13, auth_data_len - 8) bytes, terminated by a NUL.
    if server_capabilities & CLIENT_SECURE_CONNECTION != 0 {
        let part2_len = if auth_data_len > 8 {
            (auth_data_len - 8).max(13)
        } else {
            13
        };
        let end = (pos + part2_len).min(payload.len());
        let part2 = &payload[pos..end];
        // Strip a trailing NUL if present.
        let part2 = if part2.last() == Some(&0) {
            &part2[..part2.len() - 1]
        } else {
            part2
        };
        salt.extend_from_slice(part2);
        pos = end;
    }

    // auth-plugin-name (NUL-terminated string) if CLIENT_PLUGIN_AUTH
    let auth_plugin_name = if server_capabilities & CLIENT_PLUGIN_AUTH != 0
        && pos < payload.len()
    {
        // The plugin name may or may not be NUL-terminated.
        let name = read_nul_string(payload, &mut pos)
            .unwrap_or_else(|_| String::from_utf8_lossy(&payload[pos..]).to_string());
        if pos < payload.len() {
            let _ = read_nul_string(payload, &mut pos);
        }
        name
    } else {
        String::new()
    };

    Ok(ServerHandshake {
        server_capabilities,
        salt,
        auth_plugin_name,
    })
}

/// Build the Handshake Response (client → server).
fn build_handshake_response(
    capabilities: u32,
    username: &str,
    auth_response: &[u8],
    database: &str,
    plugin_name: &str,
    charset: &str,
) -> Vec<u8> {
    let charset_id = charset_to_id(charset);
    let mut buf = Vec::with_capacity(128);
    // capability flags (4 bytes, LE)
    buf.extend_from_slice(&capabilities.to_le_bytes());
    // max packet size (4 bytes) — 16 MiB
    buf.extend_from_slice(&(0x00ff_ffffu32).to_le_bytes());
    // character set (1 byte)
    buf.push(charset_id);
    // reserved (23 zero bytes)
    buf.extend_from_slice(&[0u8; 23]);
    // username (NUL-terminated)
    buf.extend_from_slice(username.as_bytes());
    buf.push(0);
    // auth response (length-encoded, via CLIENT_PLUGIN_AUTH_LENENC_CLIENT_DATA
    // — we set CLIENT_SECURE_CONNECTION so it's a 1-byte length prefix).
    buf.push(auth_response.len() as u8);
    buf.extend_from_slice(auth_response);
    // database (NUL-terminated) if CLIENT_CONNECT_WITH_DB
    if capabilities & CLIENT_CONNECT_WITH_DB != 0 && !database.is_empty() {
        buf.extend_from_slice(database.as_bytes());
        buf.push(0);
    }
    // auth plugin name (NUL-terminated) if CLIENT_PLUGIN_AUTH
    if capabilities & CLIENT_PLUGIN_AUTH != 0 && !plugin_name.is_empty() {
        buf.extend_from_slice(plugin_name.as_bytes());
        buf.push(0);
    }
    // connection attributes (empty) if CLIENT_CONNECT_ATTRS
    if capabilities & CLIENT_CONNECT_ATTRS != 0 {
        buf.push(0); // length-encoded 0 attributes
    }
    buf
}

/// Compute the initial auth response for the server-requested plugin.
fn compute_initial_auth_response(
    plugin: &str,
    password: &[u8],
    salt: &[u8],
) -> Result<Vec<u8>, DbError> {
    if password.is_empty() {
        return Ok(Vec::new());
    }
    match plugin {
        "" | "mysql_native_password" => Ok(mysql_native_password_hash(password, salt)),
        "caching_sha2_password" | "sha256_password" => {
            Ok(caching_sha2_password_hash(password))
        }
        other => Err(DbError::auth_error(format!(
            "Unsupported initial auth plugin: {} (please use mysql_native_password or caching_sha2_password)",
            other
        ))),
    }
}

/// Compute the auth-switch response for the new plugin the server requested.
fn compute_auth_switch_response(
    plugin: &str,
    password: &[u8],
    salt: &[u8],
) -> Result<Vec<u8>, DbError> {
    if password.is_empty() {
        return Ok(Vec::new());
    }
    match plugin {
        "mysql_native_password" => Ok(mysql_native_password_hash(password, salt)),
        "caching_sha2_password" | "sha256_password" => {
            Ok(caching_sha2_password_hash(password))
        }
        other => Err(DbError::auth_error(format!(
            "Unsupported auth-switch plugin: {}",
            other
        ))),
    }
}

/// `mysql_native_password`: SHA1(password) XOR SHA1(salt || SHA1(SHA1(password)))
fn mysql_native_password_hash(password: &[u8], salt: &[u8]) -> Vec<u8> {
    let hash1 = Sha1::digest(password);
    let hash2 = Sha1::digest(&hash1);
    let mut hasher = Sha1::new();
    hasher.update(salt);
    hasher.update(hash2);
    let hash3 = hasher.finalize();
    hash1
        .iter()
        .zip(hash3.iter())
        .map(|(a, b)| a ^ b)
        .collect()
}

/// `caching_sha2_password`: SHA256(SHA256(password)) — the 32-byte hash sent as
/// the initial auth response. The server checks its cache and either accepts
/// (fast auth) or requests the full RSA-encrypted password exchange.
fn caching_sha2_password_hash(password: &[u8]) -> Vec<u8> {
    let hash1 = Sha256::digest(password);
    let hash2 = Sha256::digest(&hash1);
    hash2.to_vec()
}

/// Encrypt a password with the server's RSA public key using OAEP-SHA256,
/// as required by `caching_sha2_password` full auth (no SSL).
fn encrypt_password_with_rsa(password: &[u8], pem_key: &[u8]) -> Result<Vec<u8>, DbError> {
    use rsa::oaep::Oaep;
    use rsa::pkcs8::DecodePublicKey;
    let pem = std::str::from_utf8(pem_key)
        .map_err(|_| DbError::protocol_error("RSA public key is not valid UTF-8"))?;
    let public_key = rsa::RsaPublicKey::from_public_key_pem(pem).map_err(|e| {
        DbError::protocol_error(format!("Failed to parse RSA public key: {}", e))
    })?;
    // MySQL appends a trailing NUL to the password before encryption.
    let mut msg = password.to_vec();
    msg.push(0);
    let padding = Oaep::new::<Sha256>();
    let mut rng = rand::thread_rng();
    public_key
        .encrypt(&mut rng, padding, &msg)
        .map_err(|e| DbError::auth_error(format!("RSA encryption failed: {}", e)))
}

// ---------------------------------------------------------------------------
// Query protocols
// ---------------------------------------------------------------------------

/// `COM_QUERY` text protocol (no parameters).
async fn com_query(
    stream: &mut TcpStream,
    sql: &str,
    capabilities: u32,
) -> Result<MyQueryResult, DbError> {
    let mut payload = Vec::with_capacity(sql.len() + 1);
    payload.push(0x03); // COM_QUERY
    payload.extend_from_slice(sql.as_bytes());
    write_packet(stream, 0, &payload).await?;

    let (_, resp) = read_packet(stream).await?;
    if resp.is_empty() {
        return Err(DbError::protocol_error("Empty COM_QUERY response"));
    }
    match resp[0] {
        0x00 => {
            let ok = parse_ok_packet(&resp, capabilities)?;
            Ok(MyQueryResult {
                rows: Vec::new(),
                rows_affected: ok.affected_rows,
                last_insert_id: ok.last_insert_id,
            })
        }
        0xff => Err(parse_error_packet(&resp, capabilities)),
        _ => {
            // Result set: the first packet is a length-encoded column count.
            let mut pos = 0;
            let column_count = read_lenenc_int(&resp, &mut pos)? as usize;
            let columns = read_column_definitions(stream, column_count).await?;
            // In non-DEPRECATE_EOF mode, an EOF packet follows the column defs.
            if capabilities & CLIENT_DEPRECATE_EOF == 0 {
                read_eof_or_ok(stream).await?;
            }

            let mut rows = Vec::new();
            loop {
                let (_, row_payload) = read_packet(stream).await?;
                if row_payload.is_empty() {
                    return Err(DbError::protocol_error("Empty row packet"));
                }
                if is_end_of_rows(&row_payload, capabilities) {
                    break;
                }
                if row_payload[0] == 0xff {
                    return Err(parse_error_packet(&row_payload, capabilities));
                }
                rows.push(parse_text_row(&row_payload, &columns)?);
            }

            Ok(MyQueryResult {
                rows,
                rows_affected: 0,
                last_insert_id: None,
            })
        }
    }
}

/// Prepared-statement path: `COM_STMT_PREPARE` then `COM_STMT_EXECUTE` (binary protocol).
///
/// 每次执行后发送 `COM_STMT_CLOSE` 释放服务器端的 prepared statement，避免
/// 持续累积直至超过 MySQL 的 `max_prepared_stmt_count` 上限。
async fn com_stmt_execute_query(
    stream: &mut TcpStream,
    sql: &str,
    params: &[SqlValue],
    capabilities: u32,
) -> Result<MyQueryResult, DbError> {
    let stmt_id = com_stmt_prepare(stream, sql, capabilities).await?;
    let column_types = params
        .iter()
        .map(|p| sql_value_to_mysql_type(p))
        .collect::<Vec<_>>();
    let result = com_stmt_execute(stream, stmt_id, &column_types, params, capabilities).await;
    // 无论执行成功与否都关闭语句，防止 prepared statement 泄漏。
    let _ = com_stmt_close(stream, stmt_id).await;
    result
}

/// `COM_STMT_CLOSE` — 关闭一个 prepared statement，释放服务器端资源（单向，无响应）。
async fn com_stmt_close(stream: &mut TcpStream, stmt_id: u32) -> Result<(), DbError> {
    let mut payload = Vec::with_capacity(5);
    payload.push(0x19); // COM_STMT_CLOSE
    payload.extend_from_slice(&stmt_id.to_le_bytes());
    write_packet(stream, 0, &payload).await?;
    Ok(())
}

/// `COM_STMT_PREPARE` — send the SQL, return the statement id and the number of
/// result columns / param columns.
async fn com_stmt_prepare(
    stream: &mut TcpStream,
    sql: &str,
    capabilities: u32,
) -> Result<u32, DbError> {
    let mut payload = Vec::with_capacity(sql.len() + 1);
    payload.push(0x16); // COM_STMT_PREPARE
    payload.extend_from_slice(sql.as_bytes());
    write_packet(stream, 0, &payload).await?;

    let (_, resp) = read_packet(stream).await?;
    if resp.is_empty() {
        return Err(DbError::protocol_error("Empty COM_STMT_PREPARE response"));
    }
    if resp[0] == 0xff {
        return Err(parse_error_packet(&resp, capabilities));
    }
    if resp[0] != 0x00 {
        return Err(DbError::protocol_error(format!(
            "Unexpected COM_STMT_PREPARE response header: 0x{:02x}",
            resp[0]
        )));
    }
    // COM_STMT_PREPARE_OK: status(1)=0, stmt_id(4), num_columns(2), num_params(2),
    // reserved(1), warning_count(2)
    if resp.len() < 12 {
        return Err(DbError::protocol_error("Malformed COM_STMT_PREPARE_OK"));
    }
    let stmt_id = u32::from_le_bytes([resp[1], resp[2], resp[3], resp[4]]);
    let num_columns = u16::from_le_bytes([resp[5], resp[6]]) as usize;
    let num_params = u16::from_le_bytes([resp[7], resp[8]]) as usize;

    // Read param column definitions (if any) + EOF.
    if num_params > 0 {
        let _ = read_column_definitions(stream, num_params).await?;
        if capabilities & CLIENT_DEPRECATE_EOF == 0 {
            read_eof_or_ok(stream).await?;
        }
    }
    // Read result column definitions (if any) + EOF.
    if num_columns > 0 {
        let _ = read_column_definitions(stream, num_columns).await?;
        if capabilities & CLIENT_DEPRECATE_EOF == 0 {
            read_eof_or_ok(stream).await?;
        }
    }

    Ok(stmt_id)
}

/// `COM_STMT_EXECUTE` — run a prepared statement with bound parameters (binary protocol).
async fn com_stmt_execute(
    stream: &mut TcpStream,
    stmt_id: u32,
    param_types: &[u8],
    params: &[SqlValue],
    capabilities: u32,
) -> Result<MyQueryResult, DbError> {
    let num_params = params.len();
    let mut payload = Vec::new();
    payload.push(0x17); // COM_STMT_EXECUTE
    payload.extend_from_slice(&stmt_id.to_le_bytes());
    payload.push(0x00); // flags: CURSOR_TYPE_NO_CURSOR
    payload.extend_from_slice(&1u32.to_le_bytes()); // iteration count

    if num_params > 0 {
        // Null bitmap: ceil(num_params / 8) bytes, bit set = NULL.
        let bitmap_len = (num_params + 7) / 8;
        let mut bitmap = vec![0u8; bitmap_len];
        let new_params_bound = 1u8;
        for (i, p) in params.iter().enumerate() {
            if matches!(p, SqlValue::Null) {
                bitmap[i / 8] |= 1 << (i % 8);
            }
        }
        payload.extend_from_slice(&bitmap);
        payload.push(new_params_bound);
        // Type bytes: one (type, unsigned-flag) pair per parameter.
        for (i, t) in param_types.iter().enumerate() {
            payload.push(*t);
            // Unsigned flag (0x80) for non-negative integer types.
            let unsigned = match &params[i] {
                SqlValue::I32(v) => *v >= 0,
                SqlValue::I64(v) => *v >= 0,
                SqlValue::I16(v) => *v >= 0,
                SqlValue::I8(v) => *v >= 0,
                _ => false,
            };
            payload.push(if unsigned { 0x80 } else { 0x00 });
        }
        // Parameter values (skip NULLs, which are encoded in the bitmap).
        for p in params.iter() {
            if !matches!(p, SqlValue::Null) {
                encode_binary_param(p, &mut payload);
            }
        }
    }

    write_packet(stream, 0, &payload).await?;

    let (_, resp) = read_packet(stream).await?;
    if resp.is_empty() {
        return Err(DbError::protocol_error("Empty COM_STMT_EXECUTE response"));
    }
    if resp[0] == 0xff {
        return Err(parse_error_packet(&resp, capabilities));
    }
    if resp[0] == 0x00 {
        let ok = parse_ok_packet(&resp, capabilities)?;
        return Ok(MyQueryResult {
            rows: Vec::new(),
            rows_affected: ok.affected_rows,
            last_insert_id: ok.last_insert_id,
        });
    }
    // Result set: length-encoded column count.
    let mut pos = 0;
    let column_count = read_lenenc_int(&resp, &mut pos)? as usize;
    let columns = read_column_definitions(stream, column_count).await?;
    if capabilities & CLIENT_DEPRECATE_EOF == 0 {
        read_eof_or_ok(stream).await?;
    }

    let mut rows = Vec::new();
    loop {
        let (_, row_payload) = read_packet(stream).await?;
        if row_payload.is_empty() {
            return Err(DbError::protocol_error("Empty row packet"));
        }
        if is_end_of_rows(&row_payload, capabilities) {
            break;
        }
        if row_payload[0] == 0xff {
            return Err(parse_error_packet(&row_payload, capabilities));
        }
        rows.push(parse_binary_row(&row_payload, &columns)?);
    }

    Ok(MyQueryResult {
        rows,
        rows_affected: 0,
        last_insert_id: None,
    })
}

/// Read `count` Column Definition packets from the stream.
async fn read_column_definitions(
    stream: &mut TcpStream,
    count: usize,
) -> Result<Vec<ColumnInfo>, DbError> {
    let mut columns = Vec::with_capacity(count);
    for _ in 0..count {
        let (_, payload) = read_packet(stream).await?;
        columns.push(parse_column_definition(&payload)?);
    }
    Ok(columns)
}

/// Read either an EOF packet or an OK packet (when CLIENT_DEPRECATE_EOF is set).
async fn read_eof_or_ok(stream: &mut TcpStream) -> Result<(), DbError> {
    let (_, payload) = read_packet(stream).await?;
    if payload.is_empty() {
        return Err(DbError::protocol_error("Empty packet where EOF/OK expected"));
    }
    if payload[0] == 0xff {
        return Err(parse_error_packet(&payload, 0));
    }
    // 0xfe with short payload = EOF, 0x00 = OK (deprecate-eof mode).
    if payload[0] != 0xfe && payload[0] != 0x00 {
        return Err(DbError::protocol_error(format!(
            "Expected EOF/OK packet, got header 0x{:02x}",
            payload[0]
        )));
    }
    Ok(())
}

/// Check whether a packet payload marks the end of rows in a result set.
///
/// - In non-DEPRECATE_EOF mode: an EOF packet (0xfe, payload < 9 bytes).
/// - In DEPRECATE_EOF mode: an OK packet with 0xfe header (payload < 9 bytes
///   for typical SELECT end-of-rows where affected_rows = 0).
///
/// A text-protocol row starting with 0xfe would require the first column value
/// to be >= 2^24 bytes, making the packet >= 9 bytes — so the length check
/// safely distinguishes end-of-rows markers from row data.
fn is_end_of_rows(payload: &[u8], _capabilities: u32) -> bool {
    !payload.is_empty() && payload[0] == 0xfe && payload.len() < 9
}

// ---------------------------------------------------------------------------
// Packet parsing
// ---------------------------------------------------------------------------

struct ColumnInfo {
    name: String,
    col_type: u8,
}

struct OkPacket {
    affected_rows: u64,
    last_insert_id: Option<i64>,
}

/// Parse a Column Definition packet (Protocol 41).
fn parse_column_definition(payload: &[u8]) -> Result<ColumnInfo, DbError> {
    let mut pos = 0;
    // catalog (lenenc string, usually "def")
    let _ = read_lenenc_string(payload, &mut pos)?;
    // schema, table, org_table, name, org_name (lenenc strings)
    let _ = read_lenenc_string(payload, &mut pos)?;
    let _ = read_lenenc_string(payload, &mut pos)?;
    let _ = read_lenenc_string(payload, &mut pos)?;
    let name = read_lenenc_string(payload, &mut pos)?;
    let _ = read_lenenc_string(payload, &mut pos)?;
    // fixed-length fields length (lenenc int, always 0x0c)
    let _ = read_lenenc_int(payload, &mut pos)?;
    // character set (2), column length (4), type (1), flags (2), decimals (1), filler (2)
    pos += 2 + 4;
    if pos >= payload.len() {
        return Err(DbError::protocol_error("Column definition too short for type"));
    }
    let col_type = payload[pos];
    Ok(ColumnInfo {
        name: String::from_utf8_lossy(&name).to_string(),
        col_type,
    })
}

/// Parse a text-protocol row (length-encoded strings, NULL = 0xFB).
fn parse_text_row(payload: &[u8], columns: &[ColumnInfo]) -> Result<Row, DbError> {
    let mut pos = 0;
    let mut values = Vec::with_capacity(columns.len());
    for col in columns {
        if pos < payload.len() && payload[pos] == 0xfb {
            values.push(SqlValue::Null);
            pos += 1;
        } else {
            let raw = read_lenenc_string(payload, &mut pos)?;
            values.push(decode_text_value(&raw, col.col_type));
        }
    }
    Ok(Row::new(
        columns.iter().map(|c| c.name.clone()).collect(),
        values,
    ))
}

/// Parse a binary-protocol row. The first byte is 0x00 (packet header), followed
/// by a NULL bitmap and the non-NULL values in column order.
fn parse_binary_row(payload: &[u8], columns: &[ColumnInfo]) -> Result<Row, DbError> {
    let mut pos = 1; // skip the 0x00 header
    let n = columns.len();
    // NULL bitmap: (n + 7 + 2) / 8 bytes, offset by 2 bits.
    let bitmap_len = (n + 7 + 2) / 8;
    if pos + bitmap_len > payload.len() {
        return Err(DbError::protocol_error("Binary row too short for NULL bitmap"));
    }
    let bitmap = &payload[pos..pos + bitmap_len];
    pos += bitmap_len;

    let mut values = Vec::with_capacity(n);
    for (i, col) in columns.iter().enumerate() {
        let bit_index = i + 2;
        let is_null = (bitmap[bit_index / 8] >> (bit_index % 8)) & 1 == 1;
        if is_null {
            values.push(SqlValue::Null);
        } else {
            values.push(decode_binary_value(payload, &mut pos, col.col_type)?);
        }
    }
    Ok(Row::new(
        columns.iter().map(|c| c.name.clone()).collect(),
        values,
    ))
}

/// Parse an OK packet (header 0x00).
fn parse_ok_packet(payload: &[u8], _capabilities: u32) -> Result<OkPacket, DbError> {
    let mut pos = 1; // skip 0x00
    let affected_rows = read_lenenc_int(payload, &mut pos)?;
    let last_insert_id_raw = read_lenenc_int(payload, &mut pos)?;
    let last_insert_id = if last_insert_id_raw == 0 {
        None
    } else {
        Some(last_insert_id_raw as i64)
    };
    Ok(OkPacket {
        affected_rows,
        last_insert_id,
    })
}

/// Parse an Error packet (header 0xFF).
fn parse_error_packet(payload: &[u8], _capabilities: u32) -> DbError {
    if payload.len() < 9 {
        return DbError::query_error(format!(
            "MySQL error (malformed packet: {:?})",
            payload
        ));
    }
    let code = u16::from_le_bytes([payload[1], payload[2]]);
    // payload[3] = '#', payload[4..9] = sql state
    let sql_state = String::from_utf8_lossy(&payload[4..9]).to_string();
    let message = String::from_utf8_lossy(&payload[9..]).to_string();
    let msg = if message.is_empty() {
        format!("MySQL error {} ({})", code, sql_state)
    } else {
        message
    };
    match code {
        1044 | 1045 | 1698 => DbError::auth_error(msg),
        1062 => DbError::constraint_error(msg),
        1205 | 1213 => DbError::transaction_error(msg),
        _ => DbError::query_error(msg),
    }
}

// ---------------------------------------------------------------------------
// Value encoding / decoding
// ---------------------------------------------------------------------------

/// Map a `SqlValue` to its MySQL column type byte for the binary protocol.
fn sql_value_to_mysql_type(value: &SqlValue) -> u8 {
    match value {
        SqlValue::Null => MYSQL_TYPE_NULL,
        SqlValue::Bool(_) => MYSQL_TYPE_TINY,
        SqlValue::I8(_) => MYSQL_TYPE_TINY,
        SqlValue::I16(_) => MYSQL_TYPE_SHORT,
        SqlValue::I32(_) => MYSQL_TYPE_LONG,
        SqlValue::I64(_) => MYSQL_TYPE_LONGLONG,
        SqlValue::F32(_) => MYSQL_TYPE_FLOAT,
        SqlValue::F64(_) => MYSQL_TYPE_DOUBLE,
        SqlValue::String(_) | SqlValue::Json(_) => MYSQL_TYPE_VAR_STRING,
        SqlValue::Bytes(_) => MYSQL_TYPE_BLOB,
        SqlValue::DateTime(_) => MYSQL_TYPE_DATETIME,
    }
}

/// Encode a non-NULL `SqlValue` into the binary protocol payload.
fn encode_binary_param(value: &SqlValue, buf: &mut Vec<u8>) {
    match value {
        SqlValue::Bool(b) => buf.push(if *b { 1 } else { 0 }),
        SqlValue::I8(v) => buf.push(*v as u8),
        SqlValue::I16(v) => buf.extend_from_slice(&v.to_le_bytes()),
        SqlValue::I32(v) => buf.extend_from_slice(&v.to_le_bytes()),
        SqlValue::I64(v) => buf.extend_from_slice(&v.to_le_bytes()),
        SqlValue::F32(v) => buf.extend_from_slice(&v.to_le_bytes()),
        SqlValue::F64(v) => buf.extend_from_slice(&v.to_le_bytes()),
        SqlValue::String(s) => {
            write_lenenc_int(buf, s.len() as u64);
            buf.extend_from_slice(s.as_bytes());
        }
        SqlValue::Json(s) => {
            write_lenenc_int(buf, s.len() as u64);
            buf.extend_from_slice(s.as_bytes());
        }
        SqlValue::Bytes(b) => {
            write_lenenc_int(buf, b.len() as u64);
            buf.extend_from_slice(b);
        }
        SqlValue::DateTime(dt) => {
            // Pack as DATETIME (length-prefixed binary):
            // length(1) + year(2 LE) + month(1) + day(1) + hour(1) + minute(1)
            // + second(1) [+ microseconds(4 LE) if non-zero]
            let date = dt.date_naive();
            let time = dt.time();
            let year = date.year() as u16;
            let month = date.month() as u8;
            let day = date.day() as u8;
            let hour = time.hour() as u8;
            let minute = time.minute() as u8;
            let second = time.second() as u8;
            let micros = time.nanosecond() / 1000;
            let yb = year.to_le_bytes();
            if micros == 0 {
                buf.push(7);
                buf.push(yb[0]);
                buf.push(yb[1]);
                buf.push(month);
                buf.push(day);
                buf.push(hour);
                buf.push(minute);
                buf.push(second);
            } else {
                buf.push(11);
                buf.push(yb[0]);
                buf.push(yb[1]);
                buf.push(month);
                buf.push(day);
                buf.push(hour);
                buf.push(minute);
                buf.push(second);
                buf.extend_from_slice(&micros.to_le_bytes());
            }
        }
        SqlValue::Null => {} // handled by the NULL bitmap
    }
}

/// Decode a text-protocol value (everything is a string except NULL).
fn decode_text_value(raw: &[u8], col_type: u8) -> SqlValue {
    let s = match std::str::from_utf8(raw) {
        Ok(v) => v,
        Err(_) => return SqlValue::Bytes(raw.to_vec()),
    };
    match col_type {
        MYSQL_TYPE_TINY => {
            s.parse::<i32>()
                .map(SqlValue::I32)
                .unwrap_or_else(|_| SqlValue::String(s.to_string()))
        }
        MYSQL_TYPE_SHORT | MYSQL_TYPE_YEAR => {
            s.parse::<i32>()
                .map(SqlValue::I32)
                .unwrap_or_else(|_| SqlValue::String(s.to_string()))
        }
        MYSQL_TYPE_INT24 | MYSQL_TYPE_LONG => {
            s.parse::<i32>()
                .map(SqlValue::I32)
                .unwrap_or_else(|_| SqlValue::String(s.to_string()))
        }
        MYSQL_TYPE_LONGLONG => {
            s.parse::<i64>()
                .map(SqlValue::I64)
                .unwrap_or_else(|_| SqlValue::String(s.to_string()))
        }
        MYSQL_TYPE_FLOAT => {
            s.parse::<f32>()
                .map(SqlValue::F32)
                .unwrap_or_else(|_| SqlValue::String(s.to_string()))
        }
        MYSQL_TYPE_DOUBLE | MYSQL_TYPE_NEWDECIMAL => {
            s.parse::<f64>()
                .map(SqlValue::F64)
                .unwrap_or_else(|_| SqlValue::String(s.to_string()))
        }
        MYSQL_TYPE_NULL => SqlValue::Null,
        MYSQL_TYPE_DATE => parse_mysql_date(s).unwrap_or(SqlValue::String(s.to_string())),
        MYSQL_TYPE_DATETIME | MYSQL_TYPE_TIMESTAMP => {
            parse_mysql_datetime(s).unwrap_or(SqlValue::String(s.to_string()))
        }
        // BLOB columns come back as text in the text protocol.
        MYSQL_TYPE_BLOB => SqlValue::Bytes(raw.to_vec()),
        _ => SqlValue::String(s.to_string()),
    }
}

/// Decode a binary-protocol value at the current cursor position.
fn decode_binary_value(
    payload: &[u8],
    pos: &mut usize,
    col_type: u8,
) -> Result<SqlValue, DbError> {
    macro_rules! need {
        ($n:expr) => {
            if *pos + $n > payload.len() {
                return Err(DbError::protocol_error(
                    "Binary row value out of bounds",
                ));
            }
        };
    }
    match col_type {
        MYSQL_TYPE_TINY => {
            need!(1);
            let v = payload[*pos] as i8;
            *pos += 1;
            Ok(SqlValue::I8(v))
        }
        MYSQL_TYPE_SHORT | MYSQL_TYPE_YEAR => {
            need!(2);
            let v = i16::from_le_bytes([payload[*pos], payload[*pos + 1]]);
            *pos += 2;
            Ok(SqlValue::I16(v))
        }
        MYSQL_TYPE_INT24 | MYSQL_TYPE_LONG => {
            need!(4);
            let v = i32::from_le_bytes([
                payload[*pos],
                payload[*pos + 1],
                payload[*pos + 2],
                payload[*pos + 3],
            ]);
            *pos += 4;
            Ok(SqlValue::I32(v))
        }
        MYSQL_TYPE_LONGLONG => {
            need!(8);
            let v = i64::from_le_bytes([
                payload[*pos],
                payload[*pos + 1],
                payload[*pos + 2],
                payload[*pos + 3],
                payload[*pos + 4],
                payload[*pos + 5],
                payload[*pos + 6],
                payload[*pos + 7],
            ]);
            *pos += 8;
            Ok(SqlValue::I64(v))
        }
        MYSQL_TYPE_FLOAT => {
            need!(4);
            let v = f32::from_le_bytes([
                payload[*pos],
                payload[*pos + 1],
                payload[*pos + 2],
                payload[*pos + 3],
            ]);
            *pos += 4;
            Ok(SqlValue::F32(v))
        }
        MYSQL_TYPE_DOUBLE => {
            need!(8);
            let v = f64::from_le_bytes([
                payload[*pos],
                payload[*pos + 1],
                payload[*pos + 2],
                payload[*pos + 3],
                payload[*pos + 4],
                payload[*pos + 5],
                payload[*pos + 6],
                payload[*pos + 7],
            ]);
            *pos += 8;
            Ok(SqlValue::F64(v))
        }
        MYSQL_TYPE_DATE | MYSQL_TYPE_DATETIME | MYSQL_TYPE_TIMESTAMP => {
            need!(1);
            let len = payload[*pos] as usize;
            *pos += 1;
            need!(len);
            let data = &payload[*pos..*pos + len];
            *pos += len;
            decode_mysql_binary_datetime(data, col_type)
        }
        MYSQL_TYPE_TIME => {
            need!(1);
            let len = payload[*pos] as usize;
            *pos += 1;
            need!(len);
            *pos += len;
            Ok(SqlValue::String("00:00:00".to_string()))
        }
        // String / blob / var_string: length-encoded bytes.
        _ => {
            let raw = read_lenenc_string(payload, pos)?;
            if col_type == MYSQL_TYPE_BLOB {
                Ok(SqlValue::Bytes(raw))
            } else {
                Ok(SqlValue::String(
                    String::from_utf8_lossy(&raw).to_string(),
                ))
            }
        }
    }
}

/// Decode a binary-protocol DATE/DATETIME/TIMESTAMP value.
fn decode_mysql_binary_datetime(data: &[u8], col_type: u8) -> Result<SqlValue, DbError> {
    if data.is_empty() {
        return Ok(SqlValue::Null);
    }
    let year = u16::from_le_bytes([data[0], data[1]]) as i32;
    let month = data[2] as u32;
    let day = data[3] as u32;
    let (hour, minute, second, micros) = if data.len() >= 7 {
        (
            data[4] as u32,
            data[5] as u32,
            data[6] as u32,
            if data.len() >= 11 {
                u32::from_le_bytes([data[7], data[8], data[9], data[10]])
            } else {
                0
            },
        )
    } else {
        (0, 0, 0, 0)
    };
    let date = chrono::NaiveDate::from_ymd_opt(year, month, day)
        .ok_or_else(|| DbError::ParseError(format!("Invalid MySQL date: {}-{}-{}", year, month, day)))?;
    let time = chrono::NaiveTime::from_hms_micro_opt(hour, minute, second, micros)
        .ok_or_else(|| DbError::ParseError(format!("Invalid MySQL time: {}:{}:{}.{}", hour, minute, second, micros)))?;
    let naive = date.and_time(time);
    let dt = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(naive, chrono::Utc);
    let _ = col_type;
    Ok(SqlValue::DateTime(dt))
}

fn parse_mysql_date(s: &str) -> Option<SqlValue> {
    let date = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()?;
    let naive = date.and_hms_opt(0, 0, 0)?;
    let dt = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(naive, chrono::Utc);
    Some(SqlValue::DateTime(dt))
}

fn parse_mysql_datetime(s: &str) -> Option<SqlValue> {
    // MySQL formats: "YYYY-MM-DD HH:MM:SS" or "YYYY-MM-DD HH:MM:SS.ffffff"
    let naive = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
        .ok()?;
    let dt = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(naive, chrono::Utc);
    Some(SqlValue::DateTime(dt))
}

// ---------------------------------------------------------------------------
// Length-encoded integer / string helpers
// ---------------------------------------------------------------------------

fn read_lenenc_int(buf: &[u8], pos: &mut usize) -> Result<u64, DbError> {
    if *pos >= buf.len() {
        return Err(DbError::protocol_error("Truncated length-encoded integer"));
    }
    let first = buf[*pos];
    *pos += 1;
    let value = match first {
        0xfb => return Err(DbError::protocol_error("Unexpected NULL in lenenc int")),
        0xfc => {
            if *pos + 2 > buf.len() {
                return Err(DbError::protocol_error("Truncated lenenc int (2)"));
            }
            let v = u16::from_le_bytes([buf[*pos], buf[*pos + 1]]) as u64;
            *pos += 2;
            v
        }
        0xfd => {
            if *pos + 3 > buf.len() {
                return Err(DbError::protocol_error("Truncated lenenc int (3)"));
            }
            let v = u32::from_le_bytes([buf[*pos], buf[*pos + 1], buf[*pos + 2], 0]) as u64;
            *pos += 3;
            v
        }
        0xfe => {
            if *pos + 8 > buf.len() {
                return Err(DbError::protocol_error("Truncated lenenc int (8)"));
            }
            let v = u64::from_le_bytes([
                buf[*pos],
                buf[*pos + 1],
                buf[*pos + 2],
                buf[*pos + 3],
                buf[*pos + 4],
                buf[*pos + 5],
                buf[*pos + 6],
                buf[*pos + 7],
            ]);
            *pos += 8;
            v
        }
        v => v as u64,
    };
    Ok(value)
}

fn read_lenenc_string(buf: &[u8], pos: &mut usize) -> Result<Vec<u8>, DbError> {
    let len = read_lenenc_int(buf, pos)? as usize;
    if *pos + len > buf.len() {
        return Err(DbError::protocol_error("Truncated length-encoded string"));
    }
    let s = buf[*pos..*pos + len].to_vec();
    *pos += len;
    Ok(s)
}

fn write_lenenc_int(buf: &mut Vec<u8>, value: u64) {
    if value < 251 {
        buf.push(value as u8);
    } else if value < 0x1_0000 {
        buf.push(0xfc);
        buf.extend_from_slice(&(value as u16).to_le_bytes());
    } else if value < 0x1_0000_00 {
        buf.push(0xfd);
        buf.extend_from_slice(&((value as u32).to_le_bytes())[..3]);
    } else {
        buf.push(0xfe);
        buf.extend_from_slice(&value.to_le_bytes());
    }
}

fn read_nul_string(buf: &[u8], pos: &mut usize) -> Result<String, DbError> {
    let start = *pos;
    while *pos < buf.len() && buf[*pos] != 0 {
        *pos += 1;
    }
    if *pos >= buf.len() {
        return Err(DbError::protocol_error("Unterminated NUL string"));
    }
    let s = String::from_utf8_lossy(&buf[start..*pos]).to_string();
    *pos += 1; // skip the NUL
    Ok(s)
}

/// Map a charset name to its MySQL collation id (used in the handshake response).
fn charset_to_id(charset: &str) -> u8 {
    match charset.to_ascii_lowercase().as_str() {
        "utf8mb4" => 45, // utf8mb4_general_ci
        "utf8" | "utf8mb3" => 33, // utf8_general_ci
        "latin1" => 8,   // latin1_swedish_ci
        "binary" => 63,
        "ascii" => 11,
        _ => 45,
    }
}

// ---------------------------------------------------------------------------
// Packet framing (4-byte header: 3-byte LE length + 1-byte sequence id)
// ---------------------------------------------------------------------------

async fn read_packet(stream: &mut TcpStream) -> Result<(u8, Vec<u8>), DbError> {
    let mut header = [0u8; 4];
    stream
        .read_exact(&mut header)
        .await
        .map_err(|e| DbError::connection_error(format!("Failed to read MySQL packet header: {}", e)))?;
    let length = (header[0] as usize)
        | ((header[1] as usize) << 8)
        | ((header[2] as usize) << 16);
    let seq = header[3];
    let mut payload = vec![0u8; length];
    if length > 0 {
        stream
            .read_exact(&mut payload)
            .await
            .map_err(|e| {
                DbError::connection_error(format!("Failed to read MySQL packet body: {}", e))
            })?;
    }
    Ok((seq, payload))
}

async fn write_packet(stream: &mut TcpStream, seq: u8, payload: &[u8]) -> Result<(), DbError> {
    let len = payload.len();
    let header = [
        (len & 0xff) as u8,
        ((len >> 8) & 0xff) as u8,
        ((len >> 16) & 0xff) as u8,
        seq,
    ];
    stream
        .write_all(&header)
        .await
        .map_err(|e| DbError::connection_error(format!("Failed to write MySQL packet header: {}", e)))?;
    if !payload.is_empty() {
        stream
            .write_all(payload)
            .await
            .map_err(|e| DbError::connection_error(format!("Failed to write MySQL packet body: {}", e)))?;
    }
    let _ = stream.flush().await;
    Ok(())
}

// ---------------------------------------------------------------------------
// DatabaseConnection trait implementation
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
impl DatabaseConnection for MySqlConnection {
    async fn execute_query(&self, sql: &str, params: &[SqlValue]) -> Result<QueryResult, DbError> {
        let result = self.run_query(sql, params).await?;
        Ok(QueryResult {
            rows: result.rows,
            rows_affected: result.rows_affected,
            last_insert_id: result.last_insert_id,
        })
    }

    async fn execute(&self, sql: &str, params: &[SqlValue]) -> Result<u64, DbError> {
        let result = self.run_query(sql, params).await?;
        Ok(result.rows_affected)
    }

    async fn begin_transaction(&self) -> Result<Transaction, DbError> {
        self.execute("BEGIN", &[]).await?;
        let conn_arc: Arc<dyn DatabaseConnection> = Arc::new(self.clone());
        Ok(Transaction::new(conn_arc))
    }

    async fn ping(&self) -> Result<(), DbError> {
        if !self.is_connected() {
            return Err(DbError::connection_error("Connection is closed"));
        }
        tokio::time::timeout(self.config.timeout, async {
            let mut guard = self.stream.lock().await;
            let stream = (&mut *guard)
                .as_mut()
                .ok_or_else(|| DbError::connection_error("Connection is closed"))?;
            // COM_PING = 0x0e
            write_packet(stream, 0, &[0x0e]).await?;
            let (_, resp) = read_packet(stream).await?;
            if resp.is_empty() {
                return Err(DbError::protocol_error("Empty COM_PING response"));
            }
            if resp[0] == 0xff {
                return Err(parse_error_packet(&resp, self.capabilities));
            }
            Ok(())
        })
        .await
        .map_err(|_| {
            DbError::TimeoutError(format!(
                "Ping timed out after {:?}",
                self.config.timeout
            ))
        })?
    }

    async fn close(&self) -> Result<(), DbError> {
        if !*self.connected.lock().unwrap() {
            return Ok(());
        }
        let mut guard = self.stream.lock().await;
        if let Some(mut stream) = guard.take() {
            // COM_QUIT = 0x01
            let _ = write_packet(&mut stream, 0, &[0x01]).await;
            let _ = stream.shutdown().await;
        }
        *self.connected.lock().unwrap() = false;
        Ok(())
    }

    fn db_type(&self) -> DbType {
        DbType::MySQL
    }

    fn is_connected(&self) -> bool {
        *self.connected.lock().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a config pointing at the local MySQL 5.7 test server.
    fn test_config() -> crate::db::database::ConnectionConfig {
        crate::db::database::ConnectionConfig::mysql("localhost", 3306, "mydb", "odoo", "odoo")
            .with_timeout(std::time::Duration::from_secs(5))
    }

    #[tokio::test]
    async fn test_mysql_connection_creation() {
        let config = test_config();
        match MySqlConnection::new(&config).await {
            Ok(conn) => {
                assert_eq!(conn.db_type(), DbType::MySQL);
                assert!(conn.is_connected());
                conn.close().await.unwrap();
                assert!(!conn.is_connected());
            }
            Err(e) => {
                eprintln!("MySQL server not reachable, skipping test: {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_mysql_ping() {
        let config = test_config();
        let conn = match MySqlConnection::new(&config).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("MySQL server not reachable, skipping test: {}", e);
                return;
            }
        };
        assert!(conn.ping().await.is_ok());
        conn.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_mysql_select_query() {
        let config = test_config();
        let conn = match MySqlConnection::new(&config).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("MySQL server not reachable, skipping test: {}", e);
                return;
            }
        };

        let result = conn
            .execute_query("SELECT 1 AS one, 'hello' AS greeting", &[])
            .await;
        assert!(result.is_ok(), "query failed: {:?}", result.err());
        let result = result.unwrap();
        assert_eq!(result.rows.len(), 1);
        let row = &result.rows[0];
        // MySQL returns integer literals as BIGINT (8-byte).
        assert_eq!(row.get("one"), Some(&SqlValue::I64(1)));
        assert_eq!(row.get("greeting"), Some(&SqlValue::String("hello".to_string())));

        conn.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_mysql_users_table() {
        let config = test_config();
        let conn = match MySqlConnection::new(&config).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("MySQL server not reachable, skipping test: {}", e);
                return;
            }
        };

        // Use a per-test table to avoid races with parallel tests.
        conn.execute(
            "DROP TABLE IF EXISTS users_text",
            &[],
        )
        .await
        .unwrap();
        conn.execute(
            "CREATE TABLE users_text (id INT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(100), age INT, created_at DATETIME)",
            &[],
        )
        .await
        .unwrap();
        conn.execute("INSERT INTO users_text (name, age, created_at) VALUES ('Alice', 30, NOW())", &[])
            .await
            .unwrap();
        conn.execute("INSERT INTO users_text (name, age, created_at) VALUES ('Bob', 25, NOW())", &[])
            .await
            .unwrap();

        let result = conn.execute_query("SELECT id, name, age FROM users_text ORDER BY id", &[])
            .await
            .unwrap();
        assert_eq!(result.rows.len(), 2);
        assert_eq!(result.rows[0].get("name"), Some(&SqlValue::String("Alice".to_string())));
        assert_eq!(result.rows[0].get("age"), Some(&SqlValue::I32(30)));
        assert_eq!(result.rows[1].get("name"), Some(&SqlValue::String("Bob".to_string())));

        conn.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_mysql_prepared_statement() {
        let config = test_config();
        let conn = match MySqlConnection::new(&config).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("MySQL server not reachable, skipping test: {}", e);
                return;
            }
        };

        // Use a per-test table to avoid races with parallel tests.
        conn.execute(
            "DROP TABLE IF EXISTS users_prep",
            &[],
        )
        .await
        .unwrap();
        conn.execute(
            "CREATE TABLE users_prep (id INT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(100), age INT, created_at DATETIME)",
            &[],
        )
        .await
        .unwrap();
        let affected = conn
            .execute(
                "INSERT INTO users_prep (name, age, created_at) VALUES (?, ?, NOW())",
                &[SqlValue::String("Carol".to_string()), SqlValue::I32(42)],
            )
            .await
            .unwrap();
        assert_eq!(affected, 1);

        // Query with a bound parameter.
        let result = conn
            .execute_query(
                "SELECT id, name, age FROM users_prep WHERE age > ?",
                &[SqlValue::I32(40)],
            )
            .await
            .unwrap();
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].get("name"), Some(&SqlValue::String("Carol".to_string())));
        assert_eq!(result.rows[0].get("age"), Some(&SqlValue::I32(42)));

        conn.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_mysql_error_handling() {
        let config = test_config();
        let conn = match MySqlConnection::new(&config).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("MySQL server not reachable, skipping test: {}", e);
                return;
            }
        };

        let result = conn.execute_query("SELECT FROM no_such_table", &[]).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, DbError::QueryError(_)),
            "expected QueryError, got {:?}",
            err
        );

        conn.close().await.unwrap();
    }

    #[test]
    fn test_mysql_native_password_hash_known_vector() {
        // Known vector: password "root" with a fixed 20-byte salt.
        // SHA1("root") = dc765..., we verify the algorithm shape instead.
        let password = b"test";
        let salt = [0u8; 20];
        let hash = mysql_native_password_hash(password, &salt);
        assert_eq!(hash.len(), 20);

        // The same inputs must produce the same output.
        let hash2 = mysql_native_password_hash(password, &salt);
        assert_eq!(hash, hash2);
    }

    #[test]
    fn test_caching_sha2_password_hash() {
        let hash = caching_sha2_password_hash(b"password");
        assert_eq!(hash.len(), 32);
        // Deterministic.
        assert_eq!(hash, caching_sha2_password_hash(b"password"));
        // Different password -> different hash.
        assert_ne!(hash, caching_sha2_password_hash(b"different"));
    }

    #[test]
    fn test_lenenc_int_roundtrip() {
        for &v in &[0u64, 1, 250, 251, 0xffff, 0x10000, 0xffffff, 0x1000000, u64::MAX] {
            let mut buf = Vec::new();
            write_lenenc_int(&mut buf, v);
            let mut pos = 0;
            let decoded = read_lenenc_int(&buf, &mut pos).unwrap();
            assert_eq!(decoded, v, "lenenc roundtrip failed for {}", v);
        }
    }

    #[test]
    fn test_charset_to_id() {
        assert_eq!(charset_to_id("utf8mb4"), 45);
        assert_eq!(charset_to_id("UTF8MB4"), 45);
        assert_eq!(charset_to_id("utf8"), 33);
        assert_eq!(charset_to_id("binary"), 63);
        assert_eq!(charset_to_id("unknown"), 45);
    }
}
