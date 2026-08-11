use crate::db::db_types::{SqlValue, QueryResult, DbType};
use crate::orm::model::Model;
use crate::orm::migration::{ColumnDefinition, IndexDefinition, TableDefinition};
use crate::utils::sql_safety::validate_identifier;
use std::sync::Arc;

/// 校验并规范化表名标识符。
fn safe_identifier(id: &str) -> String {
    validate_identifier(id).unwrap_or_else(|e| {
        eprintln!("[torm::sql_safety] rejected unsafe identifier: {e}");
        String::new()
    })
}

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

    // ------------------------------------------------------------------
    // GORM-style model persistence API
    // ------------------------------------------------------------------

    /// GORM-style create: INSERT the model, running the `before_create`/`after_create` hooks.
    /// Builds the INSERT from `Model::columns()` plus `created_at`/`updated_at`
    /// (set by the `before_create` hook; the table must include those columns).
    /// Automatically retrieves the auto-generated primary key and sets it via `model.set_id()`.
    pub async fn create<M: Model>(&self, model: &mut M) -> Result<(), DbError> {
        model.before_create()?;

        let had_id = model.id().is_some();

        let mut columns: Vec<(&str, SqlValue)> = Vec::new();
        if had_id {
            if let Some(id) = model.id() {
                columns.push((M::primary_key(), SqlValue::String(id)));
            }
        }
        columns.extend(model.columns());
        if let Some(ts) = model.created_at() {
            columns.push(("created_at", SqlValue::DateTime(ts)));
        }
        if let Some(ts) = model.updated_at() {
            columns.push(("updated_at", SqlValue::DateTime(ts)));
        }
        if columns.is_empty() {
            return Err(DbError::execution_error(
                "Model has no columns to insert (implement Model::columns)",
            ));
        }

        let names: Vec<&str> = columns.iter().map(|(name, _)| *name).collect();
        let values: Vec<SqlValue> = columns.into_iter().map(|(_, value)| value).collect();
        let phs = placeholders(self.db_type(), values.len());
        let table_name = safe_identifier(M::table_name());
        let primary_key = safe_identifier(M::primary_key());

        match self.db_type() {
            DbType::PostgreSQL => {
                let sql = format!(
                    "INSERT INTO {} ({}) VALUES ({}) RETURNING {}",
                    table_name,
                    names.join(", "),
                    phs.join(", "),
                    primary_key
                );
                let result = self.query(&sql, &values).await?;
                if !had_id {
                    if let Some(row) = result.rows.first() {
                        if let Some(id_val) = row.get(&primary_key) {
                            if let Some(i) = id_val.as_i64() {
                                model.set_id(i.to_string());
                            }
                        }
                    }
                }
            }
            _ => {
                let sql = format!(
                    "INSERT INTO {} ({}) VALUES ({})",
                    table_name,
                    names.join(", "),
                    phs.join(", "),
                );
                self.execute(&sql, &values).await?;
                if !had_id {
                    let id_sql = match self.db_type() {
                        DbType::MySQL => "SELECT LAST_INSERT_ID() AS id",
                        DbType::SQLite => "SELECT last_insert_rowid() AS id",
                        _ => unreachable!(),
                    };
                    let result = self.query(id_sql, &[]).await?;
                    if let Some(row) = result.rows.first() {
                        if let Some(i) = row.get("id").and_then(|v| v.as_i64()) {
                            model.set_id(i.to_string());
                        }
                    }
                }
            }
        }

        model.after_create()?;
        Ok(())
    }

    /// GORM-style first: find one model by primary key.
    pub async fn first_model<M: Model>(&self, id: &str) -> Result<Option<M>, DbError> {
        M::before_find()?;
        let sql = format!(
            "SELECT * FROM {} WHERE {} = {} LIMIT 1",
            safe_identifier(M::table_name()),
            safe_identifier(M::primary_key()),
            placeholder(self.db_type(), 1)
        );
        let result = self.query(&sql, &[SqlValue::String(id.to_string())]).await?;
        match result.rows.first() {
            Some(row) => {
                let mut model = M::from_row(row).ok_or_else(|| {
                    DbError::ParseError(format!(
                        "Failed to build {} from query row",
                        M::table_name()
                    ))
                })?;
                model.after_find()?;
                Ok(Some(model))
            }
            None => Ok(None),
        }
    }

    /// GORM-style find: load all rows of the model's table.
    pub async fn find_models<M: Model>(&self) -> Result<Vec<M>, DbError> {
        M::before_find()?;
        let sql = format!("SELECT * FROM {}", safe_identifier(M::table_name()));
        let result = self.query(&sql, &[]).await?;
        let mut models = Vec::with_capacity(result.rows.len());
        for row in &result.rows {
            let mut model = M::from_row(row).ok_or_else(|| {
                DbError::ParseError(format!(
                    "Failed to build {} from query row",
                    M::table_name()
                ))
            })?;
            model.after_find()?;
            models.push(model);
        }
        Ok(models)
    }

    /// GORM-style update: UPDATE the given columns of the model, matched by primary key.
    /// Runs the `before_update`/`after_update` hooks and refreshes `updated_at`.
    pub async fn update<M: Model>(
        &self,
        model: &mut M,
        updates: &[(&str, SqlValue)],
    ) -> Result<u64, DbError> {
        let id = model
            .id()
            .ok_or_else(|| DbError::execution_error("Model has no primary key value"))?;
        model.before_update()?;

        let mut sets: Vec<(&str, SqlValue)> = updates.to_vec();
        if let Some(ts) = model.updated_at() {
            sets.push(("updated_at", SqlValue::DateTime(ts)));
        }
        if sets.is_empty() {
            return Ok(0);
        }

        let mut sql = format!("UPDATE {} SET ", safe_identifier(M::table_name()));
        let mut values = Vec::with_capacity(sets.len() + 1);
        for (i, (column, value)) in sets.iter().enumerate() {
            if i > 0 {
                sql.push_str(", ");
            }
            sql.push_str(&safe_identifier(column));
            sql.push_str(" = ");
            sql.push_str(&placeholder(self.db_type(), i + 1));
            values.push(value.clone());
        }
        sql.push_str(&format!(
            " WHERE {} = {}",
            safe_identifier(M::primary_key()),
            placeholder(self.db_type(), sets.len() + 1)
        ));
        values.push(SqlValue::String(id));

        let affected = self.execute(&sql, &values).await?;
        model.after_update()?;
        Ok(affected)
    }

    /// GORM-style delete: DELETE the model row matched by primary key.
    /// Runs the `before_delete`/`after_delete` hooks.
    pub async fn delete<M: Model>(&self, model: &mut M) -> Result<u64, DbError> {
        let id = model
            .id()
            .ok_or_else(|| DbError::execution_error("Model has no primary key value"))?;
        model.before_delete()?;
        let sql = format!(
            "DELETE FROM {} WHERE {} = {}",
            safe_identifier(M::table_name()),
            safe_identifier(M::primary_key()),
            placeholder(self.db_type(), 1)
        );
        let affected = self.execute(&sql, &[SqlValue::String(id)]).await?;
        model.after_delete()?;
        Ok(affected)
    }

    /// GORM-style auto migration: create the model's table (if missing) and
    /// all of its indexes (primary key, `index`, `uniqueIndex`) based on the
    /// `TableDefinition` produced by `Model::schema()`.
    ///
    /// Idempotent: uses `IF NOT EXISTS` so it is safe to call on every startup.
    pub async fn auto_migrate<M: Model>(&self) -> Result<(), DbError> {
        let Some(table) = M::schema() else {
            return Err(DbError::execution_error(format!(
                "Model {} does not expose a schema (implement Model::schema or use #[derive(Model)])",
                M::table_name()
            )));
        };

        self.execute(&self.build_create_table_sql(&table), &[]).await?;

        for index in &table.indexes {
            self.execute(&self.build_create_index_sql(&table, index), &[])
                .await?;
        }
        Ok(())
    }

    /// Build a `CREATE TABLE IF NOT EXISTS` statement from a `TableDefinition`.
    fn build_create_table_sql(&self, table: &TableDefinition) -> String {
        let mut column_defs: Vec<String> = table
            .columns
            .iter()
            .map(|col| self.build_column_sql(col))
            .collect();

        let primary_keys: Vec<String> = table
            .columns
            .iter()
            .filter(|col| col.primary_key)
            .map(|col| col.name.clone())
            .collect();
        if !primary_keys.is_empty() {
            column_defs.push(format!("PRIMARY KEY ({})", primary_keys.join(", ")));
        }

        let mut sql = format!(
            "CREATE TABLE IF NOT EXISTS {} (\n{}\n)",
            safe_identifier(&table.name),
            column_defs.join(",\n")
        );

        if let Some(engine) = &table.engine {
            sql.push_str(&format!(" ENGINE={}", engine));
        }
        if let Some(charset) = &table.charset {
            sql.push_str(&format!(" CHARSET={}", charset));
        }
        sql
    }

    /// Build a column definition clause for `CREATE TABLE`.
    fn build_column_sql(&self, column: &ColumnDefinition) -> String {
        let mut sql = format!(
            "  {} {}",
            safe_identifier(&column.name),
            column.column_type.as_sql(self.db_type())
        );

        if column.primary_key && column.auto_increment {
            match self.db_type() {
                DbType::MySQL => sql.push_str(" AUTO_INCREMENT"),
                DbType::PostgreSQL => sql.push_str(" SERIAL"),
                DbType::SQLite => sql.push_str(" AUTOINCREMENT"),
            }
        }
        if !column.nullable {
            sql.push_str(" NOT NULL");
        }
        if let Some(default) = &column.default {
            sql.push_str(&format!(" DEFAULT {}", default));
        }
        if column.unique {
            sql.push_str(" UNIQUE");
        }
        sql
    }

    /// Build a `CREATE [UNIQUE] INDEX IF NOT EXISTS` statement.
    fn build_create_index_sql(&self, table: &TableDefinition, index: &IndexDefinition) -> String {
        let unique = if index.unique { "UNIQUE " } else { "" };
        format!(
            "CREATE {}INDEX IF NOT EXISTS {} ON {} ({})",
            unique,
            safe_identifier(&index.name),
            safe_identifier(&table.name),
            index.columns
                .iter()
                .map(|c| safe_identifier(c))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

/// Parameter placeholder for one position (`$1` for PostgreSQL, `?` otherwise).
fn placeholder(db_type: DbType, index: usize) -> String {
    match db_type {
        DbType::PostgreSQL => format!("${}", index),
        _ => "?".to_string(),
    }
}

/// Parameter placeholders for a statement with `count` parameters.
fn placeholders(db_type: DbType, count: usize) -> Vec<String> {
    (1..=count).map(|i| placeholder(db_type, i)).collect()
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

    #[tokio::test]
    async fn test_model_crud_sqlite() {
        use crate::db::db_types::Row;
        use crate::orm::model::Timestamps;
        use chrono::{DateTime, Utc};

        #[derive(Debug, Clone)]
        struct Pet {
            id: String,
            name: String,
            age: i32,
            timestamps: Timestamps,
        }

        impl Pet {
            fn new(name: &str, age: i32) -> Self {
                Self {
                    id: format!("pet-{}", name),
                    name: name.to_string(),
                    age,
                    timestamps: Timestamps::new(),
                }
            }
        }

        impl Model for Pet {
            fn table_name() -> &'static str {
                "pets"
            }

            fn id(&self) -> Option<String> {
                Some(self.id.clone())
            }

            fn set_id(&mut self, id: String) {
                self.id = id;
            }

            fn created_at(&self) -> Option<DateTime<Utc>> {
                self.timestamps.created_at
            }

            fn updated_at(&self) -> Option<DateTime<Utc>> {
                self.timestamps.updated_at
            }

            fn deleted_at(&self) -> Option<DateTime<Utc>> {
                self.timestamps.deleted_at
            }

            fn set_created_at(&mut self, ts: DateTime<Utc>) {
                self.timestamps.created_at = Some(ts);
            }

            fn set_updated_at(&mut self, ts: DateTime<Utc>) {
                self.timestamps.updated_at = Some(ts);
            }

            fn set_deleted_at(&mut self, ts: Option<DateTime<Utc>>) {
                self.timestamps.deleted_at = ts;
            }

            fn columns(&self) -> Vec<(&'static str, SqlValue)> {
                vec![
                    ("name", SqlValue::String(self.name.clone())),
                    ("age", SqlValue::I32(self.age)),
                ]
            }

            fn from_row(row: &Row) -> Option<Self> {
                Some(Self {
                    id: row.get("id")?.as_str()?.to_string(),
                    name: row.get("name")?.as_str()?.to_string(),
                    age: match row.get("age")? {
                        SqlValue::I32(a) => *a,
                        SqlValue::I64(a) => *a as i32,
                        _ => return None,
                    },
                    timestamps: Timestamps::new(),
                })
            }
        }

        let db = Database::sqlite(":memory:").await.unwrap();
        db.execute(
            "CREATE TABLE pets (
                id TEXT PRIMARY KEY,
                name TEXT,
                age INTEGER,
                created_at TEXT,
                updated_at TEXT
            )",
            &[],
        )
        .await
        .unwrap();

        // create — hooks set timestamps
        let mut pet = Pet::new("Rex", 3);
        db.create(&mut pet).await.unwrap();
        assert!(pet.timestamps.created_at.is_some());

        // first
        let found: Option<Pet> = db.first_model(&pet.id).await.unwrap();
        assert_eq!(found.unwrap().name, "Rex");

        // find all
        let mut pets: Vec<Pet> = db.find_models().await.unwrap();
        assert_eq!(pets.len(), 1);

        // update — hooks refresh updated_at
        db.update(&mut pets[0], &[("age", SqlValue::I32(4))])
            .await
            .unwrap();
        assert!(pets[0].timestamps.updated_at.is_some());
        let updated: Pet = db.first_model(&pet.id).await.unwrap().unwrap();
        assert_eq!(updated.age, 4);

        // delete
        db.delete(&mut pet).await.unwrap();
        let gone: Option<Pet> = db.first_model(&pet.id).await.unwrap();
        assert!(gone.is_none());
    }
}
