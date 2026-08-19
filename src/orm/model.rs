use chrono::{DateTime, Utc};
use crate::db::db_types::{Row, SqlValue};
use crate::db::error::Result;
use crate::orm::migration::TableDefinition;

/// Model trait - 所有模型需要实现此接口
pub trait Model: Send + Sync {
    /// Table name for this model
    fn table_name() -> &'static str;

    /// Primary key field name
    fn primary_key() -> &'static str {
        "id"
    }

    /// Get the primary key value
    fn id(&self) -> Option<String>;

    /// Set the primary key value
    fn set_id(&mut self, id: String);

    /// Get created_at timestamp
    fn created_at(&self) -> Option<DateTime<Utc>>;

    /// Get updated_at timestamp
    fn updated_at(&self) -> Option<DateTime<Utc>>;

    /// Get deleted_at timestamp (for soft delete)
    fn deleted_at(&self) -> Option<DateTime<Utc>>;

    /// Set created_at timestamp
    fn set_created_at(&mut self, timestamp: DateTime<Utc>);

    /// Set updated_at timestamp
    fn set_updated_at(&mut self, timestamp: DateTime<Utc>);

    /// Set deleted_at timestamp (for soft delete)
    fn set_deleted_at(&mut self, timestamp: Option<DateTime<Utc>>);

    /// Hook: Before create (synchronous version)
    fn before_create(&mut self) -> Result<()> {
        let now = Utc::now();
        self.set_created_at(now);
        self.set_updated_at(now);
        Ok(())
    }

    /// Hook: After create (synchronous version)
    fn after_create(&mut self) -> Result<()> {
        Ok(())
    }

    /// Hook: Before update (synchronous version)
    fn before_update(&mut self) -> Result<()> {
        self.set_updated_at(Utc::now());
        Ok(())
    }

    /// Hook: After update (synchronous version)
    fn after_update(&mut self) -> Result<()> {
        Ok(())
    }

    /// Hook: Before delete (synchronous version)
    fn before_delete(&mut self) -> Result<()> {
        Ok(())
    }

    /// Hook: After delete (synchronous version)
    fn after_delete(&mut self) -> Result<()> {
        Ok(())
    }

    /// Hook: Before find (synchronous version)
    fn before_find() -> Result<()> {
        Ok(())
    }

    /// Hook: After find (synchronous version)
    fn after_find(&mut self) -> Result<()> {
        Ok(())
    }

    /// Hook: Before save (create or update) (synchronous version)
    fn before_save(&mut self) -> Result<()> {
        if self.id().is_some() {
            self.before_update()
        } else {
            self.before_create()
        }
    }

    /// Hook: After save (create or update) (synchronous version)
    fn after_save(&mut self) -> Result<()> {
        if self.id().is_some() {
            self.after_update()
        } else {
            self.after_create()
        }
    }

    /// Check if model is soft deleted
    fn is_deleted(&self) -> bool {
        self.deleted_at().is_some()
    }

    /// 将声明了 `#[model(json = "path")]` 的标量字段同步到 JSON 数据列
    /// （即 `#[model(json_data = "field")]` 指向的字段，默认名为 `data`）。
    ///
    /// `updated_columns` 为本次 UPDATE 实际写入的列名。仅当其中包含已声明
    /// json 映射的列时，实现才应合并这些字段到 `data` 并返回 `Some(json)`；
    /// 否则返回 `None`，避免无关更新意外覆盖 `data`。
    ///
    /// `Database::update` 会在写入前自动调用本方法：若返回 `Some(json)`，
    /// 则把 `data` 列一并写入 UPDATE，从而保证标量列与 `data` JSON 中的
    /// 镜像字段保持一致。默认实现返回 `None`（不启用同步）。
    fn sync_json_fields(&mut self, _updated_columns: &[&str]) -> Option<serde_json::Value> {
        None
    }

    /// Persistable column/value pairs (excluding the primary key).
    /// Used by `Database::create` to build the INSERT statement.
    fn columns(&self) -> Vec<(&'static str, SqlValue)> {
        Vec::new()
    }

    /// The table schema (columns, primary key and GORM-style indexes) for this
    /// model. Used by `Database::auto_migrate` to create the table and its
    /// indexes automatically. Implemented by `#[derive(Model)]`.
    fn schema() -> Option<TableDefinition> {
        None
    }

    /// Reconstruct a model from a query result row.
    /// Used by `Database::first` / `Database::all`.
    fn from_row(row: &Row) -> Option<Self>
    where
        Self: Sized,
    {
        let _ = row;
        None
    }
}

/// 按 `a.b.c` 点分路径向一个 JSON 对象写入值；中间对象不存在时自动创建。
/// 返回是否发生写入。用于 `#[model(json = "path")]` 的自动同步。
pub fn json_set_path(obj: &mut serde_json::Map<String, serde_json::Value>, path: &str, value: serde_json::Value) -> bool {
    let segments: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        return false;
    }
    let mut cur = obj;
    for (i, seg) in segments.iter().enumerate() {
        if i + 1 == segments.len() {
            cur.insert(seg.to_string(), value);
            return true;
        }
        if !cur.contains_key(*seg) {
            cur.insert(seg.to_string(), serde_json::Value::Object(Default::default()));
        }
        let Some(next) = cur.get_mut(*seg).and_then(|v| v.as_object_mut()) else {
            return false;
        };
        cur = next;
    }
    true
}

/// 按 `a.b.c` 点分路径从 JSON 对象中移除值（含 None 场景）。返回是否发生移除。
pub fn json_remove_path(obj: &mut serde_json::Map<String, serde_json::Value>, path: &str) -> bool {
    let segments: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        return false;
    }
    let (last, parents) = segments.split_last().unwrap();
    let mut cur = obj;
    for seg in parents {
        let Some(next) = cur.get_mut(*seg).and_then(|v| v.as_object_mut()) else {
            return false;
        };
        cur = next;
    }
    cur.remove(*last).is_some()
}

/// Default timestamps model
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Timestamps {
    pub id: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
}

impl Timestamps {
    pub fn new() -> Self {
        Self {
            id: None,
            created_at: None,
            updated_at: None,
            deleted_at: None,
        }
    }

    pub fn with_id(mut self, id: String) -> Self {
        self.id = Some(id);
        self
    }

    pub fn with_created_at(mut self, timestamp: DateTime<Utc>) -> Self {
        self.created_at = Some(timestamp);
        self
    }

    pub fn with_updated_at(mut self, timestamp: DateTime<Utc>) -> Self {
        self.updated_at = Some(timestamp);
        self
    }

    pub fn with_deleted_at(mut self, timestamp: Option<DateTime<Utc>>) -> Self {
        self.deleted_at = timestamp;
        self
    }
}

impl Default for Timestamps {
    fn default() -> Self {
        Self::new()
    }
}