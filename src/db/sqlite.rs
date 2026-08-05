use crate::db::db_types::{SqlValue, Row, QueryResult, DbType};
use crate::db::database::{DatabaseConnection, DbError, Transaction};
use rusqlite::types::Value as SqliteValue;
use std::sync::{Arc, Mutex};

/// SQLite 连接 - 使用 rusqlite 作为存储后端（标准 SQLite 文件格式）
pub struct SqliteConnection {
    conn: Arc<Mutex<rusqlite::Connection>>,
    path: String,
    connected: Arc<Mutex<bool>>,
}

impl SqliteConnection {
    pub async fn new(path: &str) -> Result<Self, DbError> {
        // 使用 tokio 的阻塞任务执行打开操作，避免阻塞异步运行时
        let path_owned = path.to_string();
        let conn = tokio::task::spawn_blocking(move || {
            if path_owned == ":memory:" {
                rusqlite::Connection::open_in_memory()
            } else {
                rusqlite::Connection::open(&path_owned)
            }
        })
        .await
        .map_err(|e| DbError::connection_error(format!("Connection task failed: {}", e)))?
        .map_err(|e| DbError::connection_error(format!("Failed to open SQLite database: {}", e)))?;

        // 启用外键约束
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| DbError::execution_error(format!("Failed to enable foreign keys: {}", e)))?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            path: path.to_string(),
            connected: Arc::new(Mutex::new(true)),
        })
    }

    /// 将 SqlValue 转换为 rusqlite 值
    fn to_sqlite_value(value: &SqlValue) -> SqliteValue {
        match value {
            SqlValue::Null => SqliteValue::Null,
            SqlValue::Bool(b) => SqliteValue::Integer(if *b { 1 } else { 0 }),
            SqlValue::I8(i) => SqliteValue::Integer(*i as i64),
            SqlValue::I16(i) => SqliteValue::Integer(*i as i64),
            SqlValue::I32(i) => SqliteValue::Integer(*i as i64),
            SqlValue::I64(i) => SqliteValue::Integer(*i),
            SqlValue::F32(f) => SqliteValue::Real(*f as f64),
            SqlValue::F64(f) => SqliteValue::Real(*f),
            SqlValue::String(s) => SqliteValue::Text(s.clone()),
            SqlValue::Bytes(b) => SqliteValue::Blob(b.clone()),
            SqlValue::DateTime(dt) => {
                SqliteValue::Text(dt.format("%Y-%m-%d %H:%M:%S").to_string())
            }
            SqlValue::Json(s) => SqliteValue::Text(s.clone()),
        }
    }

    /// 将 rusqlite 值转换为 SqlValue
    fn from_sqlite_value(value: &SqliteValue) -> SqlValue {
        match value {
            SqliteValue::Null => SqlValue::Null,
            SqliteValue::Integer(i) => SqlValue::I64(*i),
            SqliteValue::Real(f) => SqlValue::F64(*f),
            SqliteValue::Text(s) => SqlValue::String(s.clone()),
            SqliteValue::Blob(b) => SqlValue::Bytes(b.clone()),
        }
    }

    /// 将参数列表转换为 rusqlite 参数引用
    fn to_sqlite_params(params: &[SqlValue]) -> Vec<rusqlite::types::Value> {
        params.iter().map(Self::to_sqlite_value).collect()
    }
}

impl Clone for SqliteConnection {
    fn clone(&self) -> Self {
        Self {
            conn: Arc::clone(&self.conn),
            path: self.path.clone(),
            connected: Arc::clone(&self.connected),
        }
    }
}

#[async_trait::async_trait]
impl DatabaseConnection for SqliteConnection {
    async fn execute_query(&self, sql: &str, params: &[SqlValue]) -> Result<QueryResult, DbError> {
        let sql = sql.to_string();
        let sqlite_params = Self::to_sqlite_params(params);

        let conn = self.conn.lock().unwrap();

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| DbError::query_error(format!("Failed to prepare statement: {}", e)))?;

        // 获取列名
        let column_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let columns = column_names.clone();

        let param_refs: Vec<&dyn rusqlite::ToSql> = sqlite_params
            .iter()
            .map(|v| v as &dyn rusqlite::ToSql)
            .collect();

        // 执行查询并收集行
        let rows_iter = stmt
            .query_map(param_refs.as_slice(), move |row| {
                let mut values = Vec::with_capacity(columns.len());
                for i in 0..columns.len() {
                    let v: SqliteValue = row.get(i).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            i,
                            rusqlite::types::Type::Null,
                            Box::new(e),
                        )
                    })?;
                    values.push(Self::from_sqlite_value(&v));
                }
                Ok(Row::new(columns.clone(), values))
            })
            .map_err(|e| DbError::query_error(format!("Query execution failed: {}", e)))?;

        let mut result_rows = Vec::new();
        for row in rows_iter {
            result_rows.push(row.map_err(|e| DbError::query_error(format!("Row error: {}", e)))?);
        }

        // 对于 SELECT，返回行数据；对于其他语句返回影响行数
        let is_select = sql.trim_start().to_uppercase().starts_with("SELECT")
            || sql.trim_start().to_uppercase().starts_with("WITH");
        let last_insert_id = if is_select { None } else { Some(conn.last_insert_rowid()) };

        Ok(QueryResult {
            rows: result_rows,
            rows_affected: rows_affected_value(&conn, &sql),
            last_insert_id,
        })
    }

    async fn execute(&self, sql: &str, params: &[SqlValue]) -> Result<u64, DbError> {
        let sql = sql.to_string();
        let sqlite_params = Self::to_sqlite_params(params);

        let conn = self.conn.lock().unwrap();

        let param_refs: Vec<&dyn rusqlite::ToSql> = sqlite_params
            .iter()
            .map(|v| v as &dyn rusqlite::ToSql)
            .collect();

        // SELECT/WITH 语句用 query 执行（返回行数），其他语句用 execute（返回影响行数）
        let trimmed = sql.trim_start();
        let upper = trimmed.to_uppercase();
        if upper.starts_with("SELECT") || upper.starts_with("WITH") {
            let mut stmt = conn
                .prepare(trimmed)
                .map_err(|e| DbError::query_error(format!("Failed to prepare statement: {}", e)))?;
            let mut rows = stmt
                .query(param_refs.as_slice())
                .map_err(|e| DbError::query_error(format!("Query execution failed: {}", e)))?;
            let mut count: u64 = 0;
            while rows
                .next()
                .map_err(|e| DbError::query_error(format!("Row error: {}", e)))?
                .is_some()
            {
                count += 1;
            }
            Ok(count)
        } else {
            let affected = conn
                .execute(trimmed, param_refs.as_slice())
                .map_err(|e| DbError::execution_error(format!("Execute failed: {}", e)))?;
            Ok(affected as u64)
        }
    }

    async fn begin_transaction(&self) -> Result<Transaction, DbError> {
        self.execute("BEGIN", &[]).await?;
        let conn_arc: Arc<dyn DatabaseConnection> = Arc::new(self.clone());
        Ok(Transaction::new(conn_arc))
    }

    async fn ping(&self) -> Result<(), DbError> {
        let connected = self.connected.lock().unwrap();
        if *connected {
            let conn = self.conn.lock().unwrap();
            conn.query_row("SELECT 1", [], |_| Ok(()))
                .map_err(|e| DbError::connection_error(format!("Ping failed: {}", e)))
        } else {
            Err(DbError::connection_error("Connection is closed"))
        }
    }

    async fn close(&self) -> Result<(), DbError> {
        let mut connected = self.connected.lock().unwrap();
        *connected = false;
        Ok(())
    }

    fn db_type(&self) -> DbType {
        DbType::SQLite
    }

    fn is_connected(&self) -> bool {
        *self.connected.lock().unwrap()
    }
}

/// 获取影响行数（通过最近执行的语句）
fn rows_affected_value(_conn: &rusqlite::Connection, _sql: &str) -> u64 {
    // execute_query 主要用于 SELECT；非 SELECT 语句的影响行数由 execute() 返回
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_and_query() {
        let conn = SqliteConnection::new(":memory:").await.unwrap();

        conn.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)", &[])
            .await
            .unwrap();

        conn.execute("INSERT INTO users VALUES (1, 'Alice', 25)", &[])
            .await
            .unwrap();

        let result = conn.execute_query("SELECT * FROM users", &[]).await.unwrap();
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].get("name"), Some(&SqlValue::String("Alice".to_string())));
    }

    #[tokio::test]
    async fn test_insert_with_params() {
        let conn = SqliteConnection::new(":memory:").await.unwrap();

        conn.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)", &[])
            .await
            .unwrap();

        let params = vec![
            SqlValue::I32(1),
            SqlValue::String("Alice".to_string()),
            SqlValue::I32(25),
        ];
        conn.execute("INSERT INTO users VALUES (?, ?, ?)", &params)
            .await
            .unwrap();

        let result = conn.execute_query("SELECT * FROM users", &[]).await.unwrap();
        assert_eq!(result.rows.len(), 1);
    }

    #[tokio::test]
    async fn test_update() {
        let conn = SqliteConnection::new(":memory:").await.unwrap();

        conn.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)", &[])
            .await
            .unwrap();
        conn.execute("INSERT INTO users VALUES (1, 'Alice', 25)", &[])
            .await
            .unwrap();

        let affected = conn.execute("UPDATE users SET age = 26 WHERE id = 1", &[])
            .await
            .unwrap();
        assert_eq!(affected, 1);

        let result = conn.execute_query("SELECT * FROM users", &[]).await.unwrap();
        assert_eq!(result.rows[0].get("age"), Some(&SqlValue::I64(26)));
    }

    #[tokio::test]
    async fn test_delete() {
        let conn = SqliteConnection::new(":memory:").await.unwrap();

        conn.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)", &[])
            .await
            .unwrap();
        conn.execute("INSERT INTO users VALUES (1, 'Alice', 25)", &[])
            .await
            .unwrap();

        let affected = conn.execute("DELETE FROM users WHERE id = 1", &[])
            .await
            .unwrap();
        assert_eq!(affected, 1);

        let result = conn.execute_query("SELECT * FROM users", &[]).await.unwrap();
        assert_eq!(result.rows.len(), 0);
    }

    #[tokio::test]
    async fn test_transaction() {
        let conn = SqliteConnection::new(":memory:").await.unwrap();

        conn.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)", &[])
            .await
            .unwrap();

        let mut transaction = conn.begin_transaction().await.unwrap();

        transaction
            .execute("INSERT INTO users VALUES (1, 'Alice', 25)", &[])
            .await
            .unwrap();
        transaction
            .execute("INSERT INTO users VALUES (2, 'Bob', 30)", &[])
            .await
            .unwrap();

        transaction.commit().await.unwrap();

        let result = conn.execute_query("SELECT * FROM users", &[]).await.unwrap();
        assert_eq!(result.rows.len(), 2);
    }

    #[tokio::test]
    async fn test_file_persistence() {
        let path = format!("/tmp/torm_test_persist_{}.db", uuid::Uuid::new_v4());
        let _ = std::fs::remove_file(&path);

        {
            let conn = SqliteConnection::new(&path).await.unwrap();
            conn.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)", &[])
                .await
                .unwrap();
            conn.execute("INSERT INTO users VALUES (1, 'Alice', 25)", &[])
                .await
                .unwrap();
            conn.execute("INSERT INTO users VALUES (2, 'Bob', 30)", &[])
                .await
                .unwrap();
        }

        let conn2 = SqliteConnection::new(&path).await.unwrap();
        let result = conn2.execute_query("SELECT * FROM users", &[]).await.unwrap();
        assert_eq!(result.rows.len(), 2);
        assert_eq!(result.rows[0].get("name"), Some(&SqlValue::String("Alice".to_string())));

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn test_file_is_standard_sqlite_format() {
        let path = format!("/tmp/torm_test_sqlitefmt_{}.db", uuid::Uuid::new_v4());
        let _ = std::fs::remove_file(&path);

        {
            let conn = SqliteConnection::new(&path).await.unwrap();
            conn.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)", &[])
                .await
                .unwrap();
            conn.execute("INSERT INTO users VALUES (1, 'Alice')", &[])
                .await
                .unwrap();
        }

        // 文件必须以标准 SQLite 魔数开头："SQLite format 3\0"
        let bytes = std::fs::read(&path).unwrap();
        assert!(
            bytes.starts_with(b"SQLite format 3\0"),
            "database file must be standard SQLite format, got: {:?}",
            &bytes[..16]
        );

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn test_insert_with_column_subset() {
        let conn = SqliteConnection::new(":memory:").await.unwrap();

        conn.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)", &[])
            .await
            .unwrap();

        // SQLite 原生支持列名子集插入
        conn.execute("INSERT INTO users (name, age) VALUES ('Alice', 25)", &[])
            .await
            .unwrap();

        let result = conn.execute_query("SELECT * FROM users", &[]).await.unwrap();
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].get("name"), Some(&SqlValue::String("Alice".to_string())));
        assert_eq!(result.rows[0].get("age"), Some(&SqlValue::I64(25)));
    }

    #[tokio::test]
    async fn test_create_table_if_not_exists() {
        let conn = SqliteConnection::new(":memory:").await.unwrap();

        conn.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)", &[])
            .await
            .unwrap();

        // SQLite 原生支持 IF NOT EXISTS
        let result = conn
            .execute("CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY, name TEXT)", &[])
            .await;
        assert!(result.is_ok());

        let result = conn.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)", &[])
            .await;
        assert!(result.is_err());
    }
}
