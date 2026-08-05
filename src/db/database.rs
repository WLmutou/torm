use crate::db::db_types::{SqlValue, QueryResult, DbType};
use std::sync::Arc;

/// 数据库连接 trait
#[async_trait::async_trait]
pub trait DatabaseConnection: Send + Sync {
    /// 执行查询并返回结果
    async fn execute_query(&self, sql: &str, params: &[SqlValue]) -> Result<QueryResult, DbError>;

    /// 执行 SQL 语句（INSERT, UPDATE, DELETE）
    async fn execute(&self, sql: &str, params: &[SqlValue]) -> Result<u64, DbError>;

    /// 开始事务
    async fn begin_transaction(&self) -> Result<Transaction, DbError>;

    /// Ping 连接
    async fn ping(&self) -> Result<(), DbError>;

    /// 关闭连接
    async fn close(&self) -> Result<(), DbError>;

    /// 获取数据库类型
    fn db_type(&self) -> DbType;

    /// 是否连接
    fn is_connected(&self) -> bool;
}

/// 数据库事务
pub struct Transaction {
    conn: Arc<dyn DatabaseConnection>,
    committed: bool,
    rolled_back: bool,
}

impl Transaction {
    pub fn new(conn: Arc<dyn DatabaseConnection>) -> Self {
        Self {
            conn,
            committed: false,
            rolled_back: false,
        }
    }

    pub async fn commit(&mut self) -> Result<(), DbError> {
        if !self.committed && !self.rolled_back {
            self.conn.execute("COMMIT", &[]).await?;
            self.committed = true;
        }
        Ok(())
    }

    pub async fn rollback(&mut self) -> Result<(), DbError> {
        if !self.committed && !self.rolled_back {
            self.conn.execute("ROLLBACK", &[]).await?;
            self.rolled_back = true;
        }
        Ok(())
    }

    pub async fn execute(&self, sql: &str, params: &[SqlValue]) -> Result<u64, DbError> {
        self.conn.execute(sql, params).await
    }

    pub async fn execute_query(&self, sql: &str, params: &[SqlValue]) -> Result<QueryResult, DbError> {
        self.conn.execute_query(sql, params).await
    }
}

impl Drop for Transaction {
    fn drop(&mut self) {
        // 自动回滚
        if !self.committed && !self.rolled_back {
            // Note: 我们不能在 drop 中异步执行，这里只是标记
            self.rolled_back = true;
        }
    }
}

/// 数据库错误类型
#[derive(Debug, Clone)]
pub enum DbError {
    ConnectionError(String),
    QueryError(String),
    ExecutionError(String),
    TransactionError(String),
    ParseError(String),
    TimeoutError(String),
    IoError(String),
    ProtocolError(String),
    AuthError(String),
    ConstraintError(String),
    PoolError(String),
    NotFound,
}

impl DbError {
    pub fn connection_error(msg: impl Into<String>) -> Self {
        DbError::ConnectionError(msg.into())
    }

    pub fn query_error(msg: impl Into<String>) -> Self {
        DbError::QueryError(msg.into())
    }

    pub fn execution_error(msg: impl Into<String>) -> Self {
        DbError::ExecutionError(msg.into())
    }

    pub fn transaction_error(msg: impl Into<String>) -> Self {
        DbError::TransactionError(msg.into())
    }

    pub fn protocol_error(msg: impl Into<String>) -> Self {
        DbError::ProtocolError(msg.into())
    }

    pub fn io_error(msg: impl Into<String>) -> Self {
        DbError::IoError(msg.into())
    }

    pub fn auth_error(msg: impl Into<String>) -> Self {
        DbError::AuthError(msg.into())
    }

    pub fn constraint_error(msg: impl Into<String>) -> Self {
        DbError::ConstraintError(msg.into())
    }

    pub fn pool_error(msg: impl Into<String>) -> Self {
        DbError::PoolError(msg.into())
    }
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbError::ConnectionError(msg) => write!(f, "Connection error: {}", msg),
            DbError::QueryError(msg) => write!(f, "Query error: {}", msg),
            DbError::ExecutionError(msg) => write!(f, "Execution error: {}", msg),
            DbError::TransactionError(msg) => write!(f, "Transaction error: {}", msg),
            DbError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            DbError::TimeoutError(msg) => write!(f, "Timeout error: {}", msg),
            DbError::IoError(msg) => write!(f, "IO error: {}", msg),
            DbError::ProtocolError(msg) => write!(f, "Protocol error: {}", msg),
            DbError::AuthError(msg) => write!(f, "Authentication error: {}", msg),
            DbError::ConstraintError(msg) => write!(f, "Constraint error: {}", msg),
            DbError::PoolError(msg) => write!(f, "Pool error: {}", msg),
            DbError::NotFound => write!(f, "Record not found"),
        }
    }
}

impl std::error::Error for DbError {}

/// 数据库连接配置
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    pub db_type: DbType,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
    pub timeout: std::time::Duration,
    pub max_connections: usize,
}

impl ConnectionConfig {
    pub fn sqlite(path: &str) -> Self {
        Self {
            db_type: DbType::SQLite,
            host: String::new(),
            port: 0,
            database: path.to_string(),
            username: String::new(),
            password: String::new(),
            timeout: std::time::Duration::from_secs(30),
            max_connections: 1,
        }
    }

    pub fn mysql(host: &str, port: u16, database: &str, username: &str, password: &str) -> Self {
        Self {
            db_type: DbType::MySQL,
            host: host.to_string(),
            port,
            database: database.to_string(),
            username: username.to_string(),
            password: password.to_string(),
            timeout: std::time::Duration::from_secs(30),
            max_connections: 10,
        }
    }

    pub fn postgresql(host: &str, port: u16, database: &str, username: &str, password: &str) -> Self {
        Self {
            db_type: DbType::PostgreSQL,
            host: host.to_string(),
            port,
            database: database.to_string(),
            username: username.to_string(),
            password: password.to_string(),
            timeout: std::time::Duration::from_secs(30),
            max_connections: 10,
        }
    }

    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_max_connections(mut self, max: usize) -> Self {
        self.max_connections = max;
        self
    }
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self::sqlite(":memory:")
    }
}

/// 连接工厂
pub struct ConnectionFactory;

impl ConnectionFactory {
    pub async fn create_connection(config: ConnectionConfig) -> Result<Arc<dyn DatabaseConnection>, DbError> {
        match config.db_type {
            DbType::SQLite => {
                use crate::db::sqlite::SqliteConnection;
                let conn = SqliteConnection::new(&config.database).await?;
                Ok(Arc::new(conn))
            }
            DbType::MySQL => {
                use crate::db::mysql::MySqlConnection;
                let conn = MySqlConnection::new(&config).await?;
                Ok(Arc::new(conn))
            }
            DbType::PostgreSQL => {
                use crate::db::postgresql::PostgresConnection;
                let conn = PostgresConnection::new(&config).await?;
                Ok(Arc::new(conn))
            }
        }
    }
}

/// Database struct - 高层 ORM 数据库接口
pub struct Database {
    conn: Arc<dyn DatabaseConnection>,
    config: ConnectionConfig,
}

impl Database {
    /// 创建新的数据库连接
    pub async fn connect(config: ConnectionConfig) -> Result<Self, DbError> {
        let conn = ConnectionFactory::create_connection(config.clone()).await?;
        Ok(Self { conn, config })
    }

    /// 创建 SQLite 数据库连接
    pub async fn sqlite(path: &str) -> Result<Self, DbError> {
        Self::connect(ConnectionConfig::sqlite(path)).await
    }

    /// 创建 MySQL 数据库连接
    pub async fn mysql(host: &str, port: u16, database: &str, username: &str, password: &str) -> Result<Self, DbError> {
        Self::connect(ConnectionConfig::mysql(host, port, database, username, password)).await
    }

    /// 创建 PostgreSQL 数据库连接
    pub async fn postgresql(host: &str, port: u16, database: &str, username: &str, password: &str) -> Result<Self, DbError> {
        Self::connect(ConnectionConfig::postgresql(host, port, database, username, password)).await
    }

    /// 执行查询
    pub async fn query(&self, sql: &str, params: &[SqlValue]) -> Result<QueryResult, DbError> {
        self.conn.execute_query(sql, params).await
    }

    /// 执行 SQL 语句
    pub async fn execute(&self, sql: &str, params: &[SqlValue]) -> Result<u64, DbError> {
        self.conn.execute(sql, params).await
    }

    /// 开始事务
    pub async fn begin_transaction(&self) -> Result<Transaction, DbError> {
        self.conn.begin_transaction().await
    }

    /// Ping 数据库
    pub async fn ping(&self) -> Result<(), DbError> {
        self.conn.ping().await
    }

    /// 获取数据库类型
    pub fn db_type(&self) -> DbType {
        self.conn.db_type()
    }

    /// 检查连接状态
    pub fn is_connected(&self) -> bool {
        self.conn.is_connected()
    }

    /// 获取配置
    pub fn config(&self) -> &ConnectionConfig {
        &self.config
    }

    /// 关闭数据库连接
    pub async fn close(&self) -> Result<(), DbError> {
        self.conn.close().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_config_sqlite() {
        let config = ConnectionConfig::sqlite("test.db");
        assert_eq!(config.db_type, DbType::SQLite);
        assert_eq!(config.database, "test.db");
    }

    #[test]
    fn test_connection_config_mysql() {
        let config = ConnectionConfig::mysql("localhost", 3306, "mydb", "user", "pass");
        assert_eq!(config.db_type, DbType::MySQL);
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 3306);
    }

    #[test]
    fn test_connection_config_postgresql() {
        let config = ConnectionConfig::postgresql("localhost", 5432, "mydb", "user", "pass");
        assert_eq!(config.db_type, DbType::PostgreSQL);
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 5432);
    }

    #[test]
    fn test_connection_config_chaining() {
        let config = ConnectionConfig::sqlite("test.db")
            .with_timeout(std::time::Duration::from_secs(60))
            .with_max_connections(5);

        assert_eq!(config.timeout.as_secs(), 60);
        assert_eq!(config.max_connections, 5);
    }

    #[test]
    fn test_database_error_display() {
        let error = DbError::connection_error("Failed to connect");
        assert!(error.to_string().contains("Connection error"));

        let error = DbError::query_error("Invalid SQL");
        assert!(error.to_string().contains("Query error"));

        let error = DbError::NotFound;
        assert_eq!(error.to_string(), "Record not found");
    }

    #[test]
    fn test_database_error_creation() {
        let error = DbError::connection_error("test");
        assert!(matches!(error, DbError::ConnectionError(_)));

        let error = DbError::query_error("test");
        assert!(matches!(error, DbError::QueryError(_)));

        let error = DbError::transaction_error("test");
        assert!(matches!(error, DbError::TransactionError(_)));
    }
}
