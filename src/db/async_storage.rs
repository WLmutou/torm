//! 异步 StorageEngine 封装
//!
//! [`StorageEngine`] 本身是纯内存、无 I/O 的同步实现。为了能安全地在
//! Tokio 异步运行时中被共享与并发访问，这里用 `Arc<tokio::sync::Mutex<_>>`
//! 将其包装为 [`AsyncStorageEngine`]，把全部 CRUD 方法暴露为 `async fn`。
//!
//! 特性：
//! - 使用 `tokio::sync::Mutex`（而非 `std::sync::Mutex`），持锁期间可安全 await。
//! - 通过 `Arc` 克隆共享同一份存储引擎，适合多任务并发读写。

use crate::db::db_types::{QueryResult, SqlValue};
use crate::db::storage::{
    StorageEngine, StorageError, TableSchema, WhereClause,
};
use std::sync::Arc;

/// 异步存储引擎：对 [`StorageEngine`] 的 `tokio::sync::Mutex` 封装。
///
/// 通过 `Arc` 克隆共享同一底层存储；所有公开方法均为 `async fn`。
#[derive(Clone)]
pub struct AsyncStorageEngine {
    inner: Arc<tokio::sync::Mutex<StorageEngine>>,
}

impl Default for AsyncStorageEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl AsyncStorageEngine {
    /// 创建一个空的异步存储引擎。
    pub fn new() -> Self {
        Self {
            inner: Arc::new(tokio::sync::Mutex::new(StorageEngine::new())),
        }
    }

    /// 从已有的同步 [`StorageEngine`] 包装为异步版本。
    pub fn from_sync(engine: StorageEngine) -> Self {
        Self {
            inner: Arc::new(tokio::sync::Mutex::new(engine)),
        }
    }

    /// 异步保存到文件（将底层引擎编码后写入）。
    pub async fn save_to_file(&self, path: &str) -> Result<(), StorageError> {
        let engine = self.inner.lock().await;
        engine.save_to_file(path)
    }

    /// 异步从文件加载存储引擎。
    pub async fn load_from_file(path: &str) -> Result<Self, StorageError> {
        let engine = StorageEngine::load_from_file(path)?;
        Ok(Self::from_sync(engine))
    }

    /// 异步创建表。
    pub async fn create_table(&self, schema: TableSchema) -> Result<(), StorageError> {
        let mut engine = self.inner.lock().await;
        engine.create_table(schema)
    }

    /// 异步删除表。
    pub async fn drop_table(&self, table_name: &str) -> Result<(), StorageError> {
        let mut engine = self.inner.lock().await;
        engine.drop_table(table_name)
    }

    /// 异步插入数据，返回新行的自增 id。
    pub async fn insert(
        &self,
        table_name: &str,
        values: Vec<SqlValue>,
    ) -> Result<u64, StorageError> {
        let mut engine = self.inner.lock().await;
        engine.insert(table_name, values)
    }

    /// 异步查询数据。
    pub async fn select(
        &self,
        table_name: &str,
        columns: Option<Vec<String>>,
        where_clause: Option<WhereClause>,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<QueryResult, StorageError> {
        let engine = self.inner.lock().await;
        engine.select(table_name, columns, where_clause, limit, offset)
    }

    /// 异步更新数据，返回受影响行数。
    pub async fn update(
        &self,
        table_name: &str,
        updates: std::collections::HashMap<String, SqlValue>,
        where_clause: Option<WhereClause>,
    ) -> Result<u64, StorageError> {
        let mut engine = self.inner.lock().await;
        engine.update(table_name, updates, where_clause)
    }

    /// 异步删除数据，返回受影响行数。
    pub async fn delete(
        &self,
        table_name: &str,
        where_clause: Option<WhereClause>,
    ) -> Result<u64, StorageError> {
        let mut engine = self.inner.lock().await;
        engine.delete(table_name, where_clause)
    }

    /// 异步获取表 schema。
    pub async fn get_table_schema(&self, table_name: &str) -> Option<TableSchema> {
        let engine = self.inner.lock().await;
        engine.get_table_schema(table_name).cloned()
    }

    /// 异步获取所有表名。
    pub async fn get_table_names(&self) -> Vec<String> {
        let engine = self.inner.lock().await;
        engine.get_table_names()
    }

    /// 异步清空全部数据（等价于重建一个空引擎）。
    pub async fn clear(&self) {
        let mut engine = self.inner.lock().await;
        *engine = StorageEngine::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::storage::{ColumnDefinition, ColumnType, TableSchema};

    fn users_schema() -> TableSchema {
        TableSchema {
            name: "users".to_string(),
            columns: vec![
                ColumnDefinition {
                    name: "id".to_string(),
                    column_type: ColumnType::Integer,
                    nullable: false,
                    default: None,
                    unique: true,
                },
                ColumnDefinition {
                    name: "name".to_string(),
                    column_type: ColumnType::Text,
                    nullable: false,
                    default: None,
                    unique: false,
                },
                ColumnDefinition {
                    name: "age".to_string(),
                    column_type: ColumnType::Integer,
                    nullable: true,
                    default: None,
                    unique: false,
                },
            ],
            primary_key: Some("id".to_string()),
        }
    }

    #[tokio::test]
    async fn test_async_crud() {
        let engine = AsyncStorageEngine::new();
        engine.create_table(users_schema()).await.unwrap();

        // insert
        let id = engine
            .insert(
                "users",
                vec![
                    SqlValue::I32(1),
                    SqlValue::String("Alice".to_string()),
                    SqlValue::I32(25),
                ],
            )
            .await
            .unwrap();
        assert_eq!(id, 1);

        // select
        let result = engine.select("users", None, None, None, None).await.unwrap();
        assert_eq!(result.rows.len(), 1);
        assert_eq!(
            result.rows[0].get("name"),
            Some(&SqlValue::String("Alice".to_string()))
        );

        // update
        let mut updates = std::collections::HashMap::new();
        updates.insert("age".to_string(), SqlValue::I32(26));
        let affected = engine.update("users", updates, None).await.unwrap();
        assert_eq!(affected, 1);

        // delete
        let affected = engine.delete("users", None).await.unwrap();
        assert_eq!(affected, 1);
        let result = engine.select("users", None, None, None, None).await.unwrap();
        assert!(result.rows.is_empty());
    }

    #[tokio::test]
    async fn test_async_concurrent_inserts() {
        let engine = AsyncStorageEngine::new();
        engine.create_table(users_schema()).await.unwrap();

        // 并发插入 50 条记录：Mutex 保证线程安全，且各自的自增 id 唯一。
        let mut handles = Vec::new();
        for i in 0..50u32 {
            let engine = engine.clone();
            handles.push(tokio::spawn(async move {
                engine
                    .insert(
                        "users",
                        vec![
                            SqlValue::I32(i as i32),
                            SqlValue::String(format!("user_{}", i)),
                            SqlValue::I32(i as i32 * 2),
                        ],
                    )
                    .await
                    .unwrap();
            }));
        }
        for h in handles {
            h.await.unwrap();
        }

        let result = engine.select("users", None, None, None, None).await.unwrap();
        assert_eq!(result.rows.len(), 50);

        // 我们插入的 `id` 列值即为 0..=49，且并发下应完整、无重复。
        let mut ids: Vec<i32> = result
            .rows
            .iter()
            .filter_map(|r| r.get("id").and_then(|v| v.as_i32()))
            .collect();
        ids.sort();
        let expected: Vec<i32> = (0..50).collect();
        assert_eq!(ids, expected);
    }

    #[tokio::test]
    async fn test_async_concurrent_reads_writes() {
        let engine = AsyncStorageEngine::new();
        engine.create_table(users_schema()).await.unwrap();

        // 预置一些数据
        for i in 0..10u32 {
            engine
                .insert(
                    "users",
                    vec![
                        SqlValue::I32(i as i32),
                        SqlValue::String(format!("user_{}", i)),
                        SqlValue::I32(i as i32),
                    ],
                )
                .await
                .unwrap();
        }

        // 并发读写：读总量 + 并发更新
        let reader = engine.clone();
        let read_handle = tokio::spawn(async move {
            let result = reader.select("users", None, None, None, None).await.unwrap();
            result.rows.len()
        });

        let mut writers = Vec::new();
        for i in 0..10u32 {
            let engine = engine.clone();
            writers.push(tokio::spawn(async move {
                let mut updates = std::collections::HashMap::new();
                updates.insert("age".to_string(), SqlValue::I32(i as i32 + 100));
                engine
                    .update(
                        "users",
                        updates,
                        Some(WhereClause::Eq("id".to_string(), SqlValue::I32(i as i32))),
                    )
                    .await
                    .unwrap();
            }));
        }

        let read_count = read_handle.await.unwrap();
        for w in writers {
            w.await.unwrap();
        }
        assert_eq!(read_count, 10);
    }

    #[tokio::test]
    async fn test_async_persistence() {
        let path = format!("/tmp/torm_async_storage_{}.tormdb", uuid::Uuid::new_v4());
        let _ = std::fs::remove_file(&path);

        {
            let engine = AsyncStorageEngine::new();
            engine.create_table(users_schema()).await.unwrap();
            engine
                .insert(
                    "users",
                    vec![
                        SqlValue::I32(1),
                        SqlValue::String("Alice".to_string()),
                        SqlValue::I32(25),
                    ],
                )
                .await
                .unwrap();
            engine.save_to_file(&path).await.unwrap();
        }

        {
            let engine = AsyncStorageEngine::load_from_file(&path).await.unwrap();
            let result = engine.select("users", None, None, None, None).await.unwrap();
            assert_eq!(result.rows.len(), 1);
            assert_eq!(
                result.rows[0].get("name"),
                Some(&SqlValue::String("Alice".to_string()))
            );
        }

        let _ = std::fs::remove_file(&path);
    }
}
