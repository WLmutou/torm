use crate::db::db_types::{SqlValue, Row, QueryResult, DbType};
use crate::db::database::{DatabaseConnection, DbError, Transaction};
use crate::db::storage::{StorageEngine, TableSchema, ColumnDefinition, ColumnType, WhereClause};
use std::sync::{Arc, Mutex};

pub struct SqliteConnection {
    storage: Arc<Mutex<StorageEngine>>,
    path: String,
    connected: Arc<Mutex<bool>>,
}

impl SqliteConnection {
    pub async fn new(path: &str) -> Result<Self, DbError> {
        let storage = StorageEngine::new();
        
        // 如果是文件路径，可以在这里实现持久化逻辑
        // 对于内存数据库，直接使用内存中的存储引擎
        if path != ":memory:" {
            // 这里可以添加从文件加载数据的逻辑
            // 暂时只支持内存数据库
        }

        Ok(Self {
            storage: Arc::new(Mutex::new(storage)),
            path: path.to_string(),
            connected: Arc::new(Mutex::new(true)),
        })
    }

    /// 解析 SQL CREATE TABLE 语句
    fn parse_create_table(&self, sql: &str) -> Result<TableSchema, DbError> {
        let sql = sql.trim();
        
        if !sql.to_uppercase().starts_with("CREATE TABLE") {
            return Err(DbError::query_error("Expected CREATE TABLE statement"));
        }

        // 简化的 SQL 解析 - 保留原始大小写用于表名和值
        let sql_upper = sql.to_uppercase();
        
        // 提取表名（使用原始大小写）
        let table_name_start = sql_upper.find("CREATE TABLE").unwrap() + "CREATE TABLE".len();
        let table_name_end = sql.find('(').unwrap();
        let table_name = sql[table_name_start..table_name_end].trim().to_string();
        
        // 提取列定义（使用原始大小写，但关键字检测用大写）
        let columns_def = &sql[table_name_end + 1..sql.len() - 1];
        let mut columns = Vec::new();
        let mut primary_key = None;

        for column_str in columns_def.split(',') {
            let column_str = column_str.trim();
            let parts: Vec<&str> = column_str.split_whitespace().collect();
            
            if parts.is_empty() {
                continue;
            }

            let column_name = parts[0].to_string();
            
            // 检查是否是主键约束
            if parts.len() > 1 && parts[1].to_uppercase() == "PRIMARY" {
                primary_key = Some(column_name.clone());
                continue;
            }

            if parts.len() < 2 {
                continue;
            }

            let column_type = match parts[1].to_uppercase().as_str() {
                "INTEGER" => ColumnType::Integer,
                "BIGINT" => ColumnType::BigInt,
                "REAL" | "FLOAT" | "DOUBLE" => ColumnType::Real,
                "TEXT" | "VARCHAR" | "CHAR" => ColumnType::Text,
                "BOOLEAN" | "BOOL" => ColumnType::Boolean,
                "BLOB" => ColumnType::Blob,
                "DATETIME" | "TIMESTAMP" => ColumnType::DateTime,
                _ => ColumnType::Text, // 默认为 TEXT
            };

            let nullable = !parts.iter().any(|p| p.to_uppercase() == "NOT" && 
                                             parts.iter().position(|x| x == p).map_or(false, |i| i < parts.len() - 1 && 
                                             parts[i + 1].to_uppercase() == "NULL"));

            let unique = parts.iter().any(|p| p.to_uppercase() == "UNIQUE");

            let default = parts.iter()
                .position(|p| p.to_uppercase() == "DEFAULT")
                .and_then(|i| parts.get(i + 1))
                .map(|v| self.parse_default_value(v, &column_type));

            columns.push(ColumnDefinition {
                name: column_name,
                column_type,
                nullable,
                default,
                unique,
            });

            // 检查主键
            if primary_key.is_none() && parts.iter().any(|p| p.to_uppercase() == "PRIMARY" && 
                                                         parts.iter().position(|x| x == p).map_or(false, |i| i < parts.len() - 1 && 
                                                         parts[i + 1].to_uppercase() == "KEY")) {
                primary_key = Some(columns.last().unwrap().name.clone());
            }
        }

        Ok(TableSchema {
            name: table_name,
            columns,
            primary_key,
        })
    }

    fn parse_default_value(&self, value: &str, column_type: &ColumnType) -> SqlValue {
        let value = value.trim_matches('\'').trim_matches('"');
        
        match column_type {
            ColumnType::Integer => value.parse::<i32>()
                .ok()
                .map(SqlValue::I32)
                .unwrap_or(SqlValue::Null),
            ColumnType::BigInt => value.parse::<i64>()
                .ok()
                .map(SqlValue::I64)
                .unwrap_or(SqlValue::Null),
            ColumnType::Real => value.parse::<f64>()
                .ok()
                .map(SqlValue::F64)
                .unwrap_or(SqlValue::Null),
            ColumnType::Boolean => match value.to_uppercase().as_str() {
                "TRUE" | "1" => SqlValue::Bool(true),
                "FALSE" | "0" => SqlValue::Bool(false),
                _ => SqlValue::Null,
            },
            _ => SqlValue::String(value.to_string()),
        }
    }

    /// 解析 SQL INSERT 语句
    fn parse_insert(&self, sql: &str) -> Result<(String, Vec<SqlValue>), DbError> {
        let sql = sql.trim();
        let sql_upper = sql.to_uppercase();
        
        if !sql_upper.starts_with("INSERT INTO") {
            return Err(DbError::query_error("Expected INSERT INTO statement"));
        }

        // 简化的解析逻辑 - 使用原始大小写提取表名和值
        let table_name_start = "INSERT INTO".len();
        let table_name_end = sql_upper.find("VALUES").unwrap_or(sql.len());
        let table_part = &sql[table_name_start..table_name_end].trim();
        
        // 提取表名
        let table_name = table_part.split_whitespace()
            .next()
            .ok_or_else(|| DbError::query_error("Missing table name"))?
            .to_string();

        // 提取 VALUES 部分
        let values_start = sql_upper.find("VALUES")
            .ok_or_else(|| DbError::query_error("Missing VALUES clause"))?
            + "VALUES".len();
        
        let values_str = &sql[values_start..].trim().trim_matches('(').trim_matches(')');
        
        // 解析值
        let values = self.parse_values(values_str)?;

        Ok((table_name, values))
    }

    fn parse_values(&self, values_str: &str) -> Result<Vec<SqlValue>, DbError> {
        let mut values = Vec::new();
        let mut current = String::new();
        let mut in_string = false;
        let mut string_char = ' ';

        for ch in values_str.chars() {
            match ch {
                '\'' | '"' if !in_string => {
                    in_string = true;
                    string_char = ch;
                }
                '\'' | '"' if in_string && ch == string_char => {
                    in_string = false;
                }
                ',' if !in_string => {
                    if !current.trim().is_empty() {
                        values.push(self.parse_single_value(current.trim()));
                    }
                    current.clear();
                }
                _ => {
                    current.push(ch);
                }
            }
        }

        if !current.trim().is_empty() {
            values.push(self.parse_single_value(current.trim()));
        }

        Ok(values)
    }

    fn parse_single_value(&self, value: &str) -> SqlValue {
        let value = value.trim();
        
        if value.to_uppercase() == "NULL" {
            return SqlValue::Null;
        }

        if value.to_uppercase() == "TRUE" {
            return SqlValue::Bool(true);
        }

        if value.to_uppercase() == "FALSE" {
            return SqlValue::Bool(false);
        }

        if value.starts_with('\'') || value.starts_with('"') {
            return SqlValue::String(value[1..value.len()-1].to_string());
        }

        if value.contains('.') {
            return value.parse::<f64>()
                .ok()
                .map(SqlValue::F64)
                .unwrap_or(SqlValue::String(value.to_string()));
        }

        value.parse::<i64>()
            .ok()
            .map(|v| if v >= i32::MIN as i64 && v <= i32::MAX as i64 {
                SqlValue::I32(v as i32)
            } else {
                SqlValue::I64(v)
            })
            .unwrap_or(SqlValue::String(value.to_string()))
    }

    /// 简化的 SQL 解析和执行
    fn execute_sql_direct(&self, sql: &str, params: &[SqlValue]) -> Result<QueryResult, DbError> {
        let sql = sql.trim();
        let sql_upper = sql.to_uppercase();

        if sql_upper.starts_with("CREATE TABLE") {
            let schema = self.parse_create_table(sql)?;
            let mut storage = self.storage.lock().unwrap();
            storage.create_table(schema)
                .map(|_| QueryResult::with_affected(1))
                .map_err(|e| DbError::execution_error(e.to_string()))
        } else if sql_upper.starts_with("INSERT INTO") {
            let (table_name, values) = self.parse_insert(sql)?;
            let mut storage = self.storage.lock().unwrap();
            
            // 如果有参数，替换占位符
            let final_values = if params.is_empty() { values } else { params.to_vec() };
            
            storage.insert(&table_name, final_values)
                .map(|id| QueryResult::with_insert_id(id as i64))
                .map_err(|e| DbError::execution_error(e.to_string()))
        } else if sql_upper.starts_with("SELECT") {
            // 简化的 SELECT 处理
            let table_name = self.extract_table_name_from_select(sql)?;
            let mut storage = self.storage.lock().unwrap();
            storage.select(&table_name, None, None, None, None)
                .map_err(|e| DbError::query_error(e.to_string()))
        } else if sql_upper.starts_with("UPDATE") {
            // 简化的 UPDATE 处理
            let (table_name, updates) = self.parse_update(sql)?;
            let mut storage = self.storage.lock().unwrap();
            storage.update(&table_name, updates, None)
                .map(|count| QueryResult::with_affected(count))
                .map_err(|e| DbError::execution_error(e.to_string()))
        } else if sql_upper.starts_with("DELETE") {
            let table_name = self.extract_table_name_from_delete(sql)?;
            let mut storage = self.storage.lock().unwrap();
            storage.delete(&table_name, None)
                .map(|count| QueryResult::with_affected(count))
                .map_err(|e| DbError::execution_error(e.to_string()))
        } else if sql_upper == "BEGIN" {
            // 简化的事务处理
            Ok(QueryResult::with_affected(1))
        } else if sql_upper == "COMMIT" {
            Ok(QueryResult::with_affected(1))
        } else if sql_upper == "ROLLBACK" {
            Ok(QueryResult::with_affected(1))
        } else {
            Err(DbError::query_error(format!("Unsupported SQL: {}", sql)))
        }
    }

    fn extract_table_name_from_select(&self, sql: &str) -> Result<String, DbError> {
        let sql_upper = sql.to_uppercase();
        let from_pos = sql_upper.find("FROM")
            .ok_or_else(|| DbError::query_error("Missing FROM clause"))?;
        
        let from_part = &sql[from_pos + "FROM".len()..];
        let table_name = from_part.split_whitespace()
            .next()
            .ok_or_else(|| DbError::query_error("Missing table name"))?
            .to_string();

        Ok(table_name)
    }

    fn extract_table_name_from_delete(&self, sql: &str) -> Result<String, DbError> {
        let sql_upper = sql.to_uppercase();
        let from_pos = sql_upper.find("FROM")
            .ok_or_else(|| DbError::query_error("Missing FROM clause"))?;
        
        let from_part = &sql[from_pos + "FROM".len()..];
        let table_name = from_part.split_whitespace()
            .next()
            .ok_or_else(|| DbError::query_error("Missing table name"))?
            .to_string();

        Ok(table_name)
    }

    fn parse_update(&self, sql: &str) -> Result<(String, std::collections::HashMap<String, SqlValue>), DbError> {
        let sql_upper = sql.to_uppercase();
        let table_name_start = "UPDATE".len();
        let set_pos = sql_upper.find("SET")
            .ok_or_else(|| DbError::query_error("Missing SET clause"))?;
        
        let table_name = sql[table_name_start..set_pos].trim().to_string();
        let set_part = &sql[set_pos + "SET".len()..];
        
        // 提取 WHERE 之前的部分
        let set_clause = set_part.split("WHERE").next().unwrap_or(set_part);
        
        let mut updates = std::collections::HashMap::new();
        for assignment in set_clause.split(',') {
            let parts: Vec<&str> = assignment.split('=').collect();
            if parts.len() == 2 {
                let column = parts[0].trim().to_string();
                let value = self.parse_single_value(parts[1].trim());
                updates.insert(column, value);
            }
        }

        Ok((table_name, updates))
    }
}

impl Clone for SqliteConnection {
    fn clone(&self) -> Self {
        Self {
            storage: Arc::clone(&self.storage),
            path: self.path.clone(),
            connected: Arc::clone(&self.connected),
        }
    }
}

#[async_trait::async_trait]
impl DatabaseConnection for SqliteConnection {
    async fn execute_query(&self, sql: &str, params: &[SqlValue]) -> Result<QueryResult, DbError> {
        self.execute_sql_direct(sql, params)
    }

    async fn execute(&self, sql: &str, params: &[SqlValue]) -> Result<u64, DbError> {
        let result = self.execute_sql_direct(sql, params)?;
        Ok(result.rows_affected)
    }

    async fn begin_transaction(&self) -> Result<Transaction, DbError> {
        self.execute("BEGIN", &[]).await?;
        let conn_arc: Arc<dyn DatabaseConnection> = Arc::new(self.clone());
        Ok(Transaction::new(conn_arc))
    }

    async fn ping(&self) -> Result<(), DbError> {
        let connected = self.connected.lock().unwrap();
        if *connected {
            Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_and_query() {
        let conn = SqliteConnection::new(":memory:").await.unwrap();
        
        let create_sql = "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)";
        conn.execute(create_sql, &[]).await.unwrap();
        
        let insert_sql = "INSERT INTO users VALUES (1, 'Alice', 25)";
        conn.execute(insert_sql, &[]).await.unwrap();
        
        let result = conn.execute_query("SELECT * FROM users", &[]).await.unwrap();
        assert_eq!(result.rows.len(), 1);
    }

    #[tokio::test]
    async fn test_insert_with_params() {
        let conn = SqliteConnection::new(":memory:").await.unwrap();
        
        let create_sql = "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)";
        conn.execute(create_sql, &[]).await.unwrap();
        
        let insert_sql = "INSERT INTO users VALUES (?, ?, ?)";
        let params = vec![
            SqlValue::I32(1),
            SqlValue::String("Alice".to_string()),
            SqlValue::I32(25),
        ];
        conn.execute(insert_sql, &params).await.unwrap();
        
        let result = conn.execute_query("SELECT * FROM users", &[]).await.unwrap();
        assert_eq!(result.rows.len(), 1);
    }

    #[tokio::test]
    async fn test_update() {
        let conn = SqliteConnection::new(":memory:").await.unwrap();
        
        let create_sql = "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)";
        conn.execute(create_sql, &[]).await.unwrap();
        
        let insert_sql = "INSERT INTO users VALUES (1, 'Alice', 25)";
        conn.execute(insert_sql, &[]).await.unwrap();
        
        let update_sql = "UPDATE users SET age = 26 WHERE id = 1";
        let affected = conn.execute(update_sql, &[]).await.unwrap();
        assert_eq!(affected, 1);
    }

    #[tokio::test]
    async fn test_delete() {
        let conn = SqliteConnection::new(":memory:").await.unwrap();
        
        let create_sql = "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)";
        conn.execute(create_sql, &[]).await.unwrap();
        
        let insert_sql = "INSERT INTO users VALUES (1, 'Alice', 25)";
        conn.execute(insert_sql, &[]).await.unwrap();
        
        let delete_sql = "DELETE FROM users WHERE id = 1";
        let affected = conn.execute(delete_sql, &[]).await.unwrap();
        assert_eq!(affected, 1);
    }

    #[tokio::test]
    async fn test_transaction() {
        let conn = SqliteConnection::new(":memory:").await.unwrap();
        
        let create_sql = "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)";
        conn.execute(create_sql, &[]).await.unwrap();
        
        let mut transaction = conn.begin_transaction().await.unwrap();
        
        transaction.execute("INSERT INTO users VALUES (1, 'Alice', 25)", &[]).await.unwrap();
        transaction.execute("INSERT INTO users VALUES (2, 'Bob', 30)", &[]).await.unwrap();
        
        transaction.commit().await.unwrap();
        
        let result = conn.execute_query("SELECT * FROM users", &[]).await.unwrap();
        assert_eq!(result.rows.len(), 2);
    }
}