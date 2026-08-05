//! PostgreSQL connection — native wire protocol implementation.
//!
//! Implemented from scratch on top of `tokio::net::TcpStream`:
//! - StartupMessage + authentication (cleartext / MD5 / SCRAM-SHA-256)
//! - Simple query protocol (`Q`) for statements without parameters
//! - Extended query protocol (Parse/Bind/Describe/Execute/Sync) for parameterized statements
//! - RowDescription / DataRow decoding, CommandComplete tags, transactions

use crate::db::db_types::{DbType, QueryResult, Row, SqlValue};
use crate::db::database::{DatabaseConnection, DbError, Transaction};
use base64::Engine as _;
use md5::Md5;
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub struct PostgresConnection {
    config: PostgresConnectionConfig,
    stream: Arc<tokio::sync::Mutex<Option<TcpStream>>>,
    connected: Arc<Mutex<bool>>,
    #[allow(dead_code)]
    backend_pid: Arc<Mutex<u32>>,
    #[allow(dead_code)]
    backend_secret: Arc<Mutex<u32>>,
}

impl Clone for PostgresConnection {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            stream: Arc::clone(&self.stream),
            connected: Arc::clone(&self.connected),
            backend_pid: Arc::clone(&self.backend_pid),
            backend_secret: Arc::clone(&self.backend_secret),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PostgresConnectionConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
    pub application_name: String,
    pub timeout: std::time::Duration,
}

impl PostgresConnectionConfig {
    pub fn new(host: &str, port: u16, username: &str, password: &str, database: &str) -> Self {
        Self {
            host: host.to_string(),
            port,
            username: username.to_string(),
            password: password.to_string(),
            database: database.to_string(),
            application_name: "torm".to_string(),
            timeout: std::time::Duration::from_secs(30),
        }
    }

    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl PostgresConnection {
    pub async fn new(config: &crate::db::database::ConnectionConfig) -> Result<Self, DbError> {
        let pg_config = PostgresConnectionConfig::new(
            &config.host,
            config.port,
            &config.username,
            &config.password,
            &config.database,
        )
        .with_timeout(config.timeout);

        let (stream, backend_pid, backend_secret) = connect_and_authenticate(&pg_config).await?;

        Ok(Self {
            config: pg_config,
            stream: Arc::new(tokio::sync::Mutex::new(Some(stream))),
            connected: Arc::new(Mutex::new(true)),
            backend_pid: Arc::new(Mutex::new(backend_pid)),
            backend_secret: Arc::new(Mutex::new(backend_secret)),
        })
    }

    /// Run a statement, choosing the protocol based on whether parameters are present.
    async fn run_query(&self, sql: &str, params: &[SqlValue]) -> Result<PgQueryResult, DbError> {
        if params.is_empty() {
            self.run_simple(sql).await
        } else {
            self.run_extended(sql, params).await
        }
    }

    /// Simple query protocol (no parameters, supports multi-statement SQL).
    async fn run_simple(&self, sql: &str) -> Result<PgQueryResult, DbError> {
        tokio::time::timeout(self.config.timeout, async {
            let mut guard = self.stream.lock().await;
            let stream = (&mut *guard)
                .as_mut()
                .ok_or_else(|| DbError::connection_error("Connection is closed"))?;
            simple_query_protocol(stream, sql).await
        })
        .await
        .map_err(|_| {
            DbError::TimeoutError(format!(
                "Query timed out after {:?}: {}",
                self.config.timeout, sql
            ))
        })?
    }

    /// Extended query protocol (parameterized statements).
    async fn run_extended(&self, sql: &str, params: &[SqlValue]) -> Result<PgQueryResult, DbError> {
        tokio::time::timeout(self.config.timeout, async {
            let mut guard = self.stream.lock().await;
            let stream = (&mut *guard)
                .as_mut()
                .ok_or_else(|| DbError::connection_error("Connection is closed"))?;
            extended_query_protocol(stream, sql, params).await
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
struct PgQueryResult {
    rows: Vec<Row>,
    rows_affected: u64,
    last_insert_id: Option<i64>,
}

/// Column metadata from a RowDescription message.
struct ColumnInfo {
    name: String,
    type_oid: u32,
}

/// SCRAM-SHA-256 authentication state carried between SASL messages.
struct ScramState {
    client_first_bare: String,
    client_nonce: String,
    server_signature_b64: String,
}

// ---------------------------------------------------------------------------
// Connection establishment and authentication
// ---------------------------------------------------------------------------

async fn connect_and_authenticate(
    config: &PostgresConnectionConfig,
) -> Result<(TcpStream, u32, u32), DbError> {
    let addr = (config.host.as_str(), config.port);
    let mut stream = tokio::time::timeout(
        config.timeout,
        TcpStream::connect(addr),
    )
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

    // StartupMessage (protocol version 3.0 = 196608)
    let mut startup = Vec::new();
    startup.extend_from_slice(&196608i32.to_be_bytes());
    for (key, value) in [
        ("user", config.username.as_str()),
        ("database", config.database.as_str()),
        ("application_name", config.application_name.as_str()),
    ] {
        startup.extend_from_slice(key.as_bytes());
        startup.push(0);
        startup.extend_from_slice(value.as_bytes());
        startup.push(0);
    }
    startup.push(0);
    write_startup(&mut stream, &startup).await?;

    let mut scram: Option<ScramState> = None;
    let mut backend_pid: u32 = 0;
    let mut backend_secret: u32 = 0;

    loop {
        let (msg_type, payload) = read_message(&mut stream).await?;
        match msg_type {
            b'R' => {
                if payload.len() < 4 {
                    return Err(DbError::protocol_error(
                        "Malformed Authentication message from server",
                    ));
                }
                let code = i32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
                match code {
                    0 => {
                        // AuthenticationOk — keep reading until ReadyForQuery
                    }
                    3 => {
                        // Cleartext password
                        let mut p = config.password.as_bytes().to_vec();
                        p.push(0);
                        write_message(&mut stream, b'p', &p).await?;
                    }
                    5 => {
                        // MD5 password: md5(md5(password || user) || salt)
                        if payload.len() < 8 {
                            return Err(DbError::protocol_error(
                                "Malformed MD5 authentication message from server",
                            ));
                        }
                        let salt = &payload[4..8];
                        let inner = Md5::digest(format!("{}{}", config.password, config.username).as_bytes());
                        let inner_hex = hex::encode(inner);
                        let mut outer_input = inner_hex.into_bytes();
                        outer_input.extend_from_slice(salt);
                        let outer = Md5::digest(&outer_input);
                        let mut p = format!("md5{}", hex::encode(outer)).into_bytes();
                        p.push(0);
                        write_message(&mut stream, b'p', &p).await?;
                    }
                    10 => {
                        // SASL — choose SCRAM-SHA-256
                        let mechanisms = parse_sasl_mechanisms(&payload)?;
                        if !mechanisms.iter().any(|m| m == "SCRAM-SHA-256") {
                            return Err(DbError::auth_error(format!(
                                "Server does not offer SCRAM-SHA-256 (offered: {})",
                                mechanisms.join(", ")
                            )));
                        }
                        let client_nonce = format!(
                            "{}{}",
                            uuid::Uuid::new_v4().simple(),
                            uuid::Uuid::new_v4().simple()
                        );
                        let client_first_bare =
                            format!("n={},r={}", escape_sasl_name(&config.username), client_nonce);
                        let client_first = format!("n,,{}", client_first_bare);
                        let mut p = Vec::new();
                        p.extend_from_slice(b"SCRAM-SHA-256");
                        p.push(0);
                        p.extend_from_slice(&(client_first.len() as i32).to_be_bytes());
                        p.extend_from_slice(client_first.as_bytes());
                        write_message(&mut stream, b'p', &p).await?;
                        scram = Some(ScramState {
                            client_first_bare,
                            client_nonce,
                            server_signature_b64: String::new(),
                        });
                    }
                    11 => {
                        // SASLContinue — compute and send the client proof
                        let state = scram.as_mut().ok_or_else(|| {
                            DbError::protocol_error("SASLContinue without SASL start")
                        })?;
                        let server_first = std::str::from_utf8(&payload)
                            .map_err(|_| DbError::protocol_error("Invalid UTF-8 in SASL message"))?;
                        let (server_nonce, salt_b64, iterations) = parse_server_first(server_first)?;
                        if !server_nonce.starts_with(&state.client_nonce) {
                            return Err(DbError::auth_error(
                                "SCRAM server nonce does not start with client nonce",
                            ));
                        }
                        let salt = base64::engine::general_purpose::STANDARD
                            .decode(salt_b64)
                            .map_err(|_| DbError::auth_error("Invalid SCRAM salt"))?;
                        let salted_password =
                            pbkdf2_sha256(config.password.as_bytes(), &salt, iterations, 32);
                        let client_key = hmac_sha256(&salted_password, b"Client Key");
                        let stored_key = Sha256::digest(&client_key);
                        let client_final_without_proof = format!("c=biws,r={}", server_nonce);
                        let auth_message = format!(
                            "{},{},{}",
                            state.client_first_bare, server_first, client_final_without_proof
                        );
                        let client_signature = hmac_sha256(&stored_key, auth_message.as_bytes());
                        let mut client_proof = client_key.to_vec();
                        for i in 0..client_proof.len() {
                            client_proof[i] ^= client_signature[i];
                        }
                        let server_key = hmac_sha256(&salted_password, b"Server Key");
                        let server_signature = hmac_sha256(&server_key, auth_message.as_bytes());
                        state.server_signature_b64 =
                            base64::engine::general_purpose::STANDARD.encode(server_signature);
                        let client_final = format!(
                            "{},p={}",
                            client_final_without_proof,
                            base64::engine::general_purpose::STANDARD.encode(&client_proof)
                        );
                        write_message(&mut stream, b'p', client_final.as_bytes()).await?;
                    }
                    12 => {
                        // SASLFinal — verify the server signature
                        let state = scram.as_mut().ok_or_else(|| {
                            DbError::protocol_error("SASLFinal without SASL start")
                        })?;
                        let server_final = std::str::from_utf8(&payload)
                            .map_err(|_| DbError::protocol_error("Invalid UTF-8 in SASL message"))?;
                        let mut verified = false;
                        for attr in server_final.split(',') {
                            if let Some(err) = attr.strip_prefix("e=") {
                                return Err(DbError::auth_error(format!(
                                    "SCRAM server error: {}",
                                    err
                                )));
                            }
                            if let Some(v) = attr.strip_prefix("v=") {
                                if v != state.server_signature_b64 {
                                    return Err(DbError::auth_error(
                                        "SCRAM server signature verification failed",
                                    ));
                                }
                                verified = true;
                            }
                        }
                        if !verified {
                            return Err(DbError::auth_error(
                                "SCRAM server final message missing v= attribute",
                            ));
                        }
                    }
                    _ => {
                        return Err(DbError::auth_error(format!(
                            "Unsupported authentication method requested by server: {}",
                            code
                        )));
                    }
                }
            }
            b'E' => return Err(parse_error_response(&payload)),
            b'K' => {
                if payload.len() >= 8 {
                    backend_pid =
                        u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
                    backend_secret =
                        u32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]);
                }
            }
            b'S' => {
                // ParameterStatus — ignored
            }
            b'Z' => break,
            _ => {
                // Ignore any other message during startup
            }
        }
    }

    Ok((stream, backend_pid, backend_secret))
}

// ---------------------------------------------------------------------------
// Query protocols
// ---------------------------------------------------------------------------

/// Simple query protocol: send `Q`, read messages until `ReadyForQuery`.
async fn simple_query_protocol(stream: &mut TcpStream, sql: &str) -> Result<PgQueryResult, DbError> {
    let mut payload = Vec::with_capacity(sql.len() + 1);
    payload.extend_from_slice(sql.as_bytes());
    payload.push(0);
    write_message(stream, b'Q', &payload).await?;

    let mut columns: Vec<ColumnInfo> = Vec::new();
    let mut rows: Vec<Row> = Vec::new();
    let mut rows_affected: u64 = 0;
    let mut last_insert_id: Option<i64> = None;

    loop {
        let (msg_type, payload) = read_message(stream).await?;
        match msg_type {
            b'T' => columns = parse_row_description(&payload)?,
            b'D' => {
                if columns.is_empty() {
                    return Err(DbError::protocol_error("DataRow without RowDescription"));
                }
                rows.push(parse_data_row(&payload, &columns)?);
            }
            b'C' => {
                let tag = String::from_utf8_lossy(&payload).to_string();
                let (affected, insert_id) = parse_command_tag(&tag);
                rows_affected = affected;
                last_insert_id = insert_id;
            }
            b'I' => {
                // EmptyQueryResponse
                rows_affected = 0;
            }
            b'E' => return Err(parse_error_response(&payload)),
            b'Z' => break,
            // 'S' ParameterStatus, 'K' BackendKeyData, 'N' NoticeResponse, 'A' Notification — ignored
            _ => {}
        }
    }

    // For statements returning rows, the affected count is the number of rows returned.
    if !columns.is_empty() {
        rows_affected = rows.len() as u64;
    }

    Ok(PgQueryResult {
        rows,
        rows_affected,
        last_insert_id,
    })
}

/// Extended query protocol: Parse/Bind/Describe/Execute/Sync (text format for params and results).
async fn extended_query_protocol(
    stream: &mut TcpStream,
    sql: &str,
    params: &[SqlValue],
) -> Result<PgQueryResult, DbError> {
    // Parse (unnamed statement)
    let mut parse = Vec::new();
    parse.push(0);
    parse.extend_from_slice(sql.as_bytes());
    parse.push(0);
    parse.extend_from_slice(&(params.len() as i16).to_be_bytes());
    for _ in 0..params.len() {
        parse.extend_from_slice(&0i32.to_be_bytes()); // let the server infer types
    }
    write_message(stream, b'P', &parse).await?;

    // Bind (unnamed portal, all params and results in text format)
    let mut bind = Vec::new();
    bind.push(0); // portal name
    bind.push(0); // statement name
    bind.extend_from_slice(&0i16.to_be_bytes()); // param format codes: all text
    bind.extend_from_slice(&(params.len() as i16).to_be_bytes());
    for param in params {
        match encode_param_text(param) {
            Some(bytes) => {
                bind.extend_from_slice(&(bytes.len() as i32).to_be_bytes());
                bind.extend_from_slice(&bytes);
            }
            None => bind.extend_from_slice(&(-1i32).to_be_bytes()), // NULL
        }
    }
    bind.extend_from_slice(&0i16.to_be_bytes()); // result format codes: all text
    write_message(stream, b'B', &bind).await?;

    // Describe portal (asks for RowDescription)
    write_message(stream, b'D', &[b'P', 0]).await?;

    // Execute
    let mut execute = Vec::new();
    execute.push(0); // portal name
    execute.extend_from_slice(&0i32.to_be_bytes()); // max rows: 0 = all
    write_message(stream, b'E', &execute).await?;

    // Sync
    write_message(stream, b'S', &[]).await?;

    let mut columns: Vec<ColumnInfo> = Vec::new();
    let mut rows: Vec<Row> = Vec::new();
    let mut rows_affected: u64 = 0;
    let mut last_insert_id: Option<i64> = None;

    loop {
        let (msg_type, payload) = read_message(stream).await?;
        match msg_type {
            b'1' => {} // ParseComplete
            b'2' => {} // BindComplete
            b'T' => columns = parse_row_description(&payload)?,
            b'D' => {
                if columns.is_empty() {
                    return Err(DbError::protocol_error("DataRow without RowDescription"));
                }
                rows.push(parse_data_row(&payload, &columns)?);
            }
            b'C' => {
                let tag = String::from_utf8_lossy(&payload).to_string();
                let (affected, insert_id) = parse_command_tag(&tag);
                rows_affected = affected;
                last_insert_id = insert_id;
            }
            b'n' => {} // NoData (statement produces no rows)
            b'E' => return Err(parse_error_response(&payload)),
            b'Z' => break,
            // 'N' NoticeResponse, 'S' ParameterStatus, 'A' Notification, 'K' BackendKeyData — ignored
            _ => {}
        }
    }

    if !columns.is_empty() {
        rows_affected = rows.len() as u64;
    }

    Ok(PgQueryResult {
        rows,
        rows_affected,
        last_insert_id,
    })
}

// ---------------------------------------------------------------------------
// Message framing
// ---------------------------------------------------------------------------

async fn write_startup(stream: &mut TcpStream, payload: &[u8]) -> Result<(), DbError> {
    let mut buf = Vec::with_capacity(4 + payload.len());
    buf.extend_from_slice(&((payload.len() + 4) as i32).to_be_bytes());
    buf.extend_from_slice(payload);
    stream
        .write_all(&buf)
        .await
        .map_err(|e| DbError::connection_error(format!("Failed to write to PostgreSQL server: {}", e)))?;
    stream
        .flush()
        .await
        .map_err(|e| DbError::connection_error(format!("Failed to flush PostgreSQL connection: {}", e)))?;
    Ok(())
}

async fn write_message(stream: &mut TcpStream, msg_type: u8, payload: &[u8]) -> Result<(), DbError> {
    let mut buf = Vec::with_capacity(1 + 4 + payload.len());
    buf.push(msg_type);
    buf.extend_from_slice(&((payload.len() + 4) as i32).to_be_bytes());
    buf.extend_from_slice(payload);
    stream
        .write_all(&buf)
        .await
        .map_err(|e| DbError::connection_error(format!("Failed to write to PostgreSQL server: {}", e)))?;
    stream
        .flush()
        .await
        .map_err(|e| DbError::connection_error(format!("Failed to flush PostgreSQL connection: {}", e)))?;
    Ok(())
}

async fn read_message(stream: &mut TcpStream) -> Result<(u8, Vec<u8>), DbError> {
    let msg_type = stream
        .read_u8()
        .await
        .map_err(|e| DbError::connection_error(format!("Failed to read from PostgreSQL server: {}", e)))?;
    let len = stream
        .read_i32()
        .await
        .map_err(|e| DbError::connection_error(format!("Failed to read from PostgreSQL server: {}", e)))?;
    if len < 4 {
        return Err(DbError::protocol_error(format!(
            "Invalid PostgreSQL message length: {}",
            len
        )));
    }
    let mut payload = vec![0u8; (len - 4) as usize];
    stream
        .read_exact(&mut payload)
        .await
        .map_err(|e| DbError::connection_error(format!("Failed to read from PostgreSQL server: {}", e)))?;
    Ok((msg_type, payload))
}

// ---------------------------------------------------------------------------
// Message parsing
// ---------------------------------------------------------------------------

fn read_i16(payload: &[u8], pos: &mut usize) -> Result<i16, DbError> {
    if *pos + 2 > payload.len() {
        return Err(DbError::protocol_error("Unexpected end of PostgreSQL message"));
    }
    let bytes = [payload[*pos], payload[*pos + 1]];
    *pos += 2;
    Ok(i16::from_be_bytes(bytes))
}

fn read_i32(payload: &[u8], pos: &mut usize) -> Result<i32, DbError> {
    if *pos + 4 > payload.len() {
        return Err(DbError::protocol_error("Unexpected end of PostgreSQL message"));
    }
    let bytes = [payload[*pos], payload[*pos + 1], payload[*pos + 2], payload[*pos + 3]];
    *pos += 4;
    Ok(i32::from_be_bytes(bytes))
}

fn read_cstring(payload: &[u8], pos: &mut usize) -> Result<String, DbError> {
    let start = *pos;
    while *pos < payload.len() && payload[*pos] != 0 {
        *pos += 1;
    }
    if *pos >= payload.len() {
        return Err(DbError::protocol_error("Unterminated string in PostgreSQL message"));
    }
    let s = std::str::from_utf8(&payload[start..*pos])
        .map_err(|_| DbError::protocol_error("Invalid UTF-8 in PostgreSQL message"))?
        .to_string();
    *pos += 1;
    Ok(s)
}

fn parse_row_description(payload: &[u8]) -> Result<Vec<ColumnInfo>, DbError> {
    let mut pos = 0;
    let field_count = read_i16(payload, &mut pos)?;
    let mut columns = Vec::with_capacity(field_count.max(0) as usize);
    for _ in 0..field_count.max(0) {
        let name = read_cstring(payload, &mut pos)?;
        let _table_oid = read_i32(payload, &mut pos)?;
        let _attnum = read_i16(payload, &mut pos)?;
        let type_oid = read_i32(payload, &mut pos)? as u32;
        let _typlen = read_i16(payload, &mut pos)?;
        let _typmod = read_i32(payload, &mut pos)?;
        let _format = read_i16(payload, &mut pos)?;
        columns.push(ColumnInfo { name, type_oid });
    }
    Ok(columns)
}

fn parse_data_row(payload: &[u8], columns: &[ColumnInfo]) -> Result<Row, DbError> {
    let mut pos = 0;
    let field_count = read_i16(payload, &mut pos)?;
    if field_count as usize != columns.len() {
        return Err(DbError::protocol_error(format!(
            "DataRow field count {} does not match RowDescription column count {}",
            field_count,
            columns.len()
        )));
    }
    let mut values = Vec::with_capacity(columns.len());
    for column in columns {
        let len = read_i32(payload, &mut pos)?;
        if len < 0 {
            values.push(SqlValue::Null);
        } else {
            let len = len as usize;
            if pos + len > payload.len() {
                return Err(DbError::protocol_error("DataRow value out of bounds"));
            }
            values.push(decode_value(&payload[pos..pos + len], column.type_oid)?);
            pos += len;
        }
    }
    Ok(Row::new(
        columns.iter().map(|c| c.name.clone()).collect(),
        values,
    ))
}

fn parse_command_tag(tag: &str) -> (u64, Option<i64>) {
    // CommandComplete payloads are NUL-terminated strings
    let tag = tag.trim_end_matches('\0').trim();
    let mut parts = tag.split_whitespace();
    let command = parts.next().unwrap_or("");
    let numbers: Vec<u64> = parts.filter_map(|p| p.parse::<u64>().ok()).collect();
    match (command, numbers.as_slice()) {
        // "INSERT oid rows"
        ("INSERT", [oid, rows]) => (
            *rows,
            if *oid > 0 { Some(*oid as i64) } else { None },
        ),
        // "SELECT rows", "UPDATE rows", "DELETE rows", "MERGE rows", ...
        (_, [n, ..]) => (*n, None),
        _ => (0, None),
    }
}

fn parse_error_response(payload: &[u8]) -> DbError {
    let mut pos = 0;
    let mut code = String::new();
    let mut message = String::new();
    while pos < payload.len() {
        let field_type = payload[pos];
        pos += 1;
        if field_type == 0 {
            break;
        }
        let start = pos;
        while pos < payload.len() && payload[pos] != 0 {
            pos += 1;
        }
        let field = String::from_utf8_lossy(&payload[start..pos]).to_string();
        pos += 1;
        match field_type {
            b'C' => code = field,
            b'M' => message = field,
            _ => {}
        }
    }
    let msg = if message.is_empty() {
        format!("PostgreSQL error (code {})", code)
    } else {
        message
    };
    if code.starts_with("23") {
        DbError::constraint_error(msg)
    } else if code.starts_with("08") || code.starts_with("57") {
        DbError::connection_error(msg)
    } else if code.starts_with("28") {
        DbError::auth_error(msg)
    } else {
        DbError::query_error(msg)
    }
}

fn parse_sasl_mechanisms(payload: &[u8]) -> Result<Vec<String>, DbError> {
    let mut mechanisms = Vec::new();
    let mut start = 0;
    for (i, &b) in payload.iter().enumerate() {
        if b == 0 {
            if i > start {
                let m = std::str::from_utf8(&payload[start..i])
                    .map_err(|_| DbError::protocol_error("Invalid UTF-8 in SASL mechanisms"))?;
                mechanisms.push(m.to_string());
            }
            start = i + 1;
        }
    }
    Ok(mechanisms)
}

/// Parse the SCRAM server-first-message: `r=<nonce>,s=<salt>,i=<iterations>`.
fn parse_server_first(msg: &str) -> Result<(String, &str, u32), DbError> {
    let mut server_nonce: Option<String> = None;
    let mut salt_b64: Option<&str> = None;
    let mut iterations: Option<u32> = None;
    for attr in msg.split(',') {
        if let Some(v) = attr.strip_prefix("r=") {
            server_nonce = Some(v.to_string());
        } else if let Some(v) = attr.strip_prefix("s=") {
            salt_b64 = Some(v);
        } else if let Some(v) = attr.strip_prefix("i=") {
            iterations = Some(
                v.parse::<u32>()
                    .map_err(|_| DbError::auth_error("Invalid SCRAM iteration count"))?,
            );
        }
    }
    Ok((
        server_nonce.ok_or_else(|| DbError::auth_error("SCRAM server message missing r="))?,
        salt_b64.ok_or_else(|| DbError::auth_error("SCRAM server message missing s="))?,
        iterations.ok_or_else(|| DbError::auth_error("SCRAM server message missing i="))?,
    ))
}

fn escape_sasl_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        match c {
            ',' => out.push_str("=2C"),
            '=' => out.push_str("=3D"),
            _ => out.push(c),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Value encoding / decoding (text format)
// ---------------------------------------------------------------------------

/// Encode a parameter in PostgreSQL text format. `None` means SQL NULL.
fn encode_param_text(value: &SqlValue) -> Option<Vec<u8>> {
    match value {
        SqlValue::Null => None,
        SqlValue::Bool(b) => Some(if *b { b"t".to_vec() } else { b"f".to_vec() }),
        SqlValue::I8(v) => Some(v.to_string().into_bytes()),
        SqlValue::I16(v) => Some(v.to_string().into_bytes()),
        SqlValue::I32(v) => Some(v.to_string().into_bytes()),
        SqlValue::I64(v) => Some(v.to_string().into_bytes()),
        SqlValue::F32(v) => Some(pg_float_text(*v as f64).into_bytes()),
        SqlValue::F64(v) => Some(pg_float_text(*v).into_bytes()),
        SqlValue::String(s) => Some(s.clone().into_bytes()),
        SqlValue::Bytes(b) => Some(format!("\\x{}", hex::encode(b)).into_bytes()),
        SqlValue::DateTime(dt) => Some(dt.to_rfc3339().into_bytes()),
        SqlValue::Json(s) => Some(s.clone().into_bytes()),
    }
}

fn pg_float_text(v: f64) -> String {
    if v.is_nan() {
        "NaN".to_string()
    } else if v == f64::INFINITY {
        "Infinity".to_string()
    } else if v == f64::NEG_INFINITY {
        "-Infinity".to_string()
    } else {
        v.to_string()
    }
}

/// Decode a text-format value using its column type OID.
fn decode_value(raw: &[u8], type_oid: u32) -> Result<SqlValue, DbError> {
    let s = std::str::from_utf8(raw)
        .map_err(|_| DbError::protocol_error("Invalid UTF-8 in value"))?;
    let value = match type_oid {
        // bool
        16 => SqlValue::Bool(s == "t" || s == "true" || s == "1"),
        // int8 / int2 / int4
        20 => SqlValue::I64(parse_number(s)?),
        21 => SqlValue::I16(parse_number(s)?),
        23 => SqlValue::I32(parse_number(s)?),
        // float4 / float8
        700 => SqlValue::F32(parse_pg_float(s)? as f32),
        701 => SqlValue::F64(parse_pg_float(s)?),
        // bytea
        17 => SqlValue::Bytes(decode_bytea(s)?),
        // json / jsonb
        114 | 3802 => SqlValue::Json(s.to_string()),
        // date / timestamp / timestamptz
        1082 => SqlValue::DateTime(parse_pg_date(s)?),
        1114 => SqlValue::DateTime(parse_pg_timestamp(s)?),
        1184 => SqlValue::DateTime(parse_pg_timestamptz(s)?),
        // numeric — parse as f64 when possible, otherwise keep the text
        1700 => match parse_pg_float(s) {
            Ok(f) => SqlValue::F64(f),
            Err(_) => SqlValue::String(s.to_string()),
        },
        // text-like types: text, varchar, bpchar, name, uuid, inet, cidr, macaddr,
        // interval, time, timetz, money, oid, enum, xml, citext, ...
        _ => SqlValue::String(s.to_string()),
    };
    Ok(value)
}

fn parse_number<T: std::str::FromStr>(s: &str) -> Result<T, DbError> {
    s.parse()
        .map_err(|_| DbError::ParseError(format!("Invalid numeric value: {}", s)))
}

fn parse_pg_float(s: &str) -> Result<f64, DbError> {
    match s {
        "NaN" => Ok(f64::NAN),
        "Infinity" => Ok(f64::INFINITY),
        "-Infinity" => Ok(f64::NEG_INFINITY),
        _ => s
            .parse()
            .map_err(|_| DbError::ParseError(format!("Invalid float value: {}", s))),
    }
}

fn decode_bytea(s: &str) -> Result<Vec<u8>, DbError> {
    if let Some(hex_part) = s.strip_prefix("\\x") {
        hex::decode(hex_part)
            .map_err(|_| DbError::ParseError(format!("Invalid bytea value: {}", s)))
    } else {
        // Legacy escape format — return the raw bytes as-is
        Ok(s.as_bytes().to_vec())
    }
}

fn parse_pg_date(s: &str) -> Result<chrono::DateTime<chrono::Utc>, DbError> {
    let date = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| DbError::ParseError(format!("Invalid date value: {}", s)))?;
    let naive = date.and_hms_opt(0, 0, 0).unwrap();
    Ok(chrono::DateTime::from_naive_utc_and_offset(
        naive,
        chrono::Utc,
    ))
}

fn parse_pg_timestamp(s: &str) -> Result<chrono::DateTime<chrono::Utc>, DbError> {
    let naive = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f")
        .map_err(|_| DbError::ParseError(format!("Invalid timestamp value: {}", s)))?;
    Ok(chrono::DateTime::from_naive_utc_and_offset(
        naive,
        chrono::Utc,
    ))
}

fn parse_pg_timestamptz(s: &str) -> Result<chrono::DateTime<chrono::Utc>, DbError> {
    match s {
        "infinity" => return Ok(chrono::DateTime::<chrono::Utc>::MAX_UTC),
        "-infinity" => return Ok(chrono::DateTime::<chrono::Utc>::MIN_UTC),
        _ => {}
    }

    // PG output depends on DateStyle, e.g. "2026-08-05 06:34:27.672855+00",
    // "2026-08-05 06:34:27.672855+00:00", "2026-08-05 06:34:27-05:30", "...Z", or no offset.
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f") {
        // No offset suffix — treat as UTC
        return Ok(chrono::DateTime::from_naive_utc_and_offset(
            naive,
            chrono::Utc,
        ));
    }

    // Offset suffix present — the last '+'/'-' starts the offset (after the time part)
    let offset_pos = s.rfind('+').max(s.rfind('-')).ok_or_else(|| {
        DbError::ParseError(format!("Invalid timestamptz value: {}", s))
    })?;
    let offset_secs = parse_pg_offset(&s[offset_pos..])?;
    let naive = chrono::NaiveDateTime::parse_from_str(&s[..offset_pos], "%Y-%m-%d %H:%M:%S%.f")
        .map_err(|_| DbError::ParseError(format!("Invalid timestamptz value: {}", s)))?;
    // local time − offset = UTC
    let utc_naive = naive - chrono::Duration::seconds(offset_secs as i64);
    Ok(chrono::DateTime::from_naive_utc_and_offset(
        utc_naive,
        chrono::Utc,
    ))
}

/// Parse a PostgreSQL timezone offset suffix: "+00", "+0000", "+00:00", "-05:30", ...
fn parse_pg_offset(off: &str) -> Result<i32, DbError> {
    let negative = off.starts_with('-');
    let digits: String = off[1..].chars().filter(|c| *c != ':').collect();
    if digits.len() != 2 && digits.len() != 4 {
        return Err(DbError::ParseError(format!(
            "Invalid timezone offset: {}",
            off
        )));
    }
    let hours: i32 = digits[0..2]
        .parse()
        .map_err(|_| DbError::ParseError(format!("Invalid timezone offset: {}", off)))?;
    let minutes: i32 = if digits.len() == 4 {
        digits[2..4]
            .parse()
            .map_err(|_| DbError::ParseError(format!("Invalid timezone offset: {}", off)))?
    } else {
        0
    };
    let total = hours * 3600 + minutes * 60;
    Ok(if negative { -total } else { total })
}

// ---------------------------------------------------------------------------
// Crypto helpers (HMAC-SHA256 / PBKDF2, used by SCRAM-SHA-256)
// ---------------------------------------------------------------------------

fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    const BLOCK_SIZE: usize = 64;
    let mut key = key.to_vec();
    if key.len() > BLOCK_SIZE {
        key = Sha256::digest(&key).to_vec();
    }
    key.resize(BLOCK_SIZE, 0);

    let mut ipad = [0x36u8; BLOCK_SIZE];
    let mut opad = [0x5cu8; BLOCK_SIZE];
    for i in 0..BLOCK_SIZE {
        ipad[i] ^= key[i];
        opad[i] ^= key[i];
    }

    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(data);
    let inner_hash = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_hash);

    let mut result = [0u8; 32];
    result.copy_from_slice(&outer.finalize());
    result
}

fn pbkdf2_sha256(password: &[u8], salt: &[u8], iterations: u32, dk_len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(dk_len);
    let mut block_index: u32 = 1;
    while out.len() < dk_len {
        let mut block_input = Vec::with_capacity(salt.len() + 4);
        block_input.extend_from_slice(salt);
        block_input.extend_from_slice(&block_index.to_be_bytes());
        let mut u = hmac_sha256(password, &block_input);
        let mut t = u;
        for _ in 1..iterations {
            u = hmac_sha256(password, &u);
            for i in 0..u.len() {
                t[i] ^= u[i];
            }
        }
        out.extend_from_slice(&t);
        block_index += 1;
    }
    out.truncate(dk_len);
    out
}

// ---------------------------------------------------------------------------
// DatabaseConnection trait implementation
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
impl DatabaseConnection for PostgresConnection {
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
        self.run_simple("SELECT 1").await.map(|_| ())
    }

    async fn close(&self) -> Result<(), DbError> {
        if !*self.connected.lock().unwrap() {
            return Ok(());
        }
        let mut guard = self.stream.lock().await;
        if let Some(mut stream) = guard.take() {
            // Terminate message
            let _ = write_message(&mut stream, b'X', &[]).await;
            let _ = stream.shutdown().await;
        }
        *self.connected.lock().unwrap() = false;
        Ok(())
    }

    fn db_type(&self) -> DbType {
        DbType::PostgreSQL
    }

    fn is_connected(&self) -> bool {
        *self.connected.lock().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_postgres_connection_creation() {
        let config = crate::db::database::ConnectionConfig::postgresql(
            "localhost",
            5432,
            "test",
            "user",
            "pass",
        )
        .with_timeout(std::time::Duration::from_secs(2));
        match PostgresConnection::new(&config).await {
            Ok(conn) => {
                assert_eq!(conn.db_type(), DbType::PostgreSQL);
                assert!(conn.is_connected());
                conn.close().await.unwrap();
                assert!(!conn.is_connected());
            }
            Err(e) => {
                // No PostgreSQL server available in the test environment — skip.
                eprintln!("PostgreSQL server not reachable, skipping test: {}", e);
            }
        }
    }

    #[test]
    fn test_parse_command_tag() {
        assert_eq!(parse_command_tag("SELECT 5"), (5, None));
        assert_eq!(parse_command_tag("INSERT 0 1"), (1, None));
        assert_eq!(parse_command_tag("INSERT 12345 2"), (2, Some(12345)));
        assert_eq!(parse_command_tag("UPDATE 3"), (3, None));
        assert_eq!(parse_command_tag("DELETE 0"), (0, None));
        assert_eq!(parse_command_tag("BEGIN"), (0, None));
        assert_eq!(parse_command_tag("CREATE TABLE"), (0, None));
        // CommandComplete payloads arrive NUL-terminated
        assert_eq!(parse_command_tag("INSERT 0 1\0"), (1, None));
        assert_eq!(parse_command_tag("UPDATE 1\0"), (1, None));
        assert_eq!(parse_command_tag("SELECT 2\0"), (2, None));
    }

    #[test]
    fn test_escape_sasl_name() {
        assert_eq!(escape_sasl_name("simple"), "simple");
        assert_eq!(escape_sasl_name("a,b=c"), "a=2Cb=3Dc");
    }

    #[test]
    fn test_hmac_sha256_known_vector() {
        // RFC 4231 test case 1: key = 0x0b * 20, data = "Hi There"
        let key = [0x0bu8; 20];
        let digest = hmac_sha256(&key, b"Hi There");
        let expected = "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7";
        assert_eq!(hex::encode(digest), expected);
    }

    #[test]
    fn test_pbkdf2_sha256_known_vector() {
        // RFC 7914 (PBKDF2-HMAC-SHA256) test vector: P="password", S="salt", c=1, dkLen=32
        let dk = pbkdf2_sha256(b"password", b"salt", 1, 32);
        let expected = "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b";
        assert_eq!(hex::encode(dk), expected);
    }

    #[test]
    fn test_pg_float_text() {
        assert_eq!(pg_float_text(f64::NAN), "NaN");
        assert_eq!(pg_float_text(f64::INFINITY), "Infinity");
        assert_eq!(pg_float_text(f64::NEG_INFINITY), "-Infinity");
        assert_eq!(pg_float_text(1.5), "1.5");
    }

    #[test]
    fn test_parse_pg_offset() {
        assert_eq!(parse_pg_offset("+00").unwrap(), 0);
        assert_eq!(parse_pg_offset("+0000").unwrap(), 0);
        assert_eq!(parse_pg_offset("+00:00").unwrap(), 0);
        assert_eq!(parse_pg_offset("+05:30").unwrap(), 19800);
        assert_eq!(parse_pg_offset("-0530").unwrap(), -19800);
    }

    #[test]
    fn test_parse_pg_timestamptz() {
        // PG DateStyle ISO output uses a compact "+00" offset
        let dt = parse_pg_timestamptz("2026-08-05 06:34:27.672855+00").unwrap();
        assert_eq!(
            dt.format("%Y-%m-%d %H:%M:%S%.6f").to_string(),
            "2026-08-05 06:34:27.672855"
        );

        // Explicit offset is converted to UTC
        let dt = parse_pg_timestamptz("2026-08-05 06:34:27+05:30").unwrap();
        assert_eq!(
            dt.format("%Y-%m-%d %H:%M:%S").to_string(),
            "2026-08-05 01:04:27"
        );

        // No offset is treated as UTC
        let dt = parse_pg_timestamptz("2026-08-05 06:34:27.5").unwrap();
        assert_eq!(
            dt.format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
            "2026-08-05 06:34:27.500"
        );
    }

    #[test]
    fn test_parse_data_row_null() {
        let columns = vec![ColumnInfo {
            name: "a".to_string(),
            type_oid: 23,
        }];
        // one field, length -1 (NULL)
        let mut payload = Vec::new();
        payload.extend_from_slice(&1i16.to_be_bytes());
        payload.extend_from_slice(&(-1i32).to_be_bytes());
        let row = parse_data_row(&payload, &columns).unwrap();
        assert_eq!(row.values[0], SqlValue::Null);
    }
}
