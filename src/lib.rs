// ============================================================
// TORM - Tokio ORM Library
// 模块结构：
//   db/          - 数据库层（驱动、连接、存储引擎、SQL 实现）
//   orm/         - ORM 层（模型、查询构建器、关联、迁移）
//   utils/       - 工具层（无外部依赖的简化实现）
//   monitoring/  - 监控层（日志、性能监控）
// ============================================================

// 数据库层
pub mod db;
// ORM 层
pub mod orm;
// 工具层
pub mod utils;
// 监控层
pub mod monitoring;

// Core exports - 数据库层
pub use db::db_types::{SqlValue, Row, QueryResult, DbType};
pub use db::database::{DatabaseConnection, ConnectionConfig, ConnectionFactory, Transaction, DbError, Database};
pub use db::sqlite::SqliteConnection;
pub use db::mysql::MySqlConnection;
pub use db::postgresql::PostgresConnection;
pub use db::storage::{StorageEngine, TableSchema, ColumnDefinition as StorageColumnDefinition, ColumnType as StorageColumnType, WhereClause, StorageError};
pub use db::driver::{DBDriver, Dsn};
pub use db::error::{Result, TormError};
pub use db::pool::Pool;

// Core exports - ORM 层
pub use orm::model::{Model, Timestamps};
#[doc(inline)]
pub use torm_derive::Model;
pub use orm::query::{Query, QueryBuilder, QueryExecutor, SqlStatement};
pub use orm::advanced_query::{AdvancedQuery, JoinType, JoinClause, AggFunction, AggregationClause, OrderClause, Pagination};
pub use orm::relations::{Relation, RelationType, BelongsTo, HasOne, HasMany, ManyToMany, PreloadBuilder};
pub use orm::migration::{ColumnType, ColumnDefinition, IndexDefinition, TableDefinition, Migration, MigrationOperation, ForeignKeyDefinition, Migrator, MigrationStatus};

// Core exports - 工具层
pub use utils::simple_pool::{SimplePool, PoolStatus};
pub use utils::simple_lru::SimpleLruCache;
pub use utils::simple_error::{SimpleError, SimpleResult};
pub use utils::simple_uuid::{SimpleUuid, IdGenerator};
pub use utils::sql_safety::{SqlSanitizer, validate_identifier, quote_identifier, escape_string, contains_injection_pattern};

// Core exports - 监控层
pub use monitoring::logger::{Logger, LogLevel, LogEntry, ConsoleLogger, FileLogger, CompositeLogger, QueryTracer, LogManager, PerformanceMonitor, PerformanceStats};
pub use monitoring::performance::{QueryCache, CacheStats, BatchOperation, PreparedStatementCache, ConnectionPoolMonitor, PerformanceConfig, PerformanceManager};

// Re-exports for convenience
pub use chrono;
pub use serde_json::Value as Json;
pub use uuid::Uuid;

// Include tests
#[cfg(test)]
mod torm_tests;
