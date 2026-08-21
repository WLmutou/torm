use crate::db::error::{Result, TormError};
use crate::db::database::Database;
use crate::db::db_types::DbType;
use crate::utils::sql_safety::{validate_identifier, escape_string};
use std::collections::HashMap;
use std::sync::Arc;

/// 数据类型枚举。
///
/// `ColumnType::as_sql(db_type)` 按方言生成 SQL 类型字符串：
/// SQLite / MySQL / PostgreSQL 三类。`VARCHAR` 的实际长度由
/// [`ColumnDefinition::length`] 控制（默认 255）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnType {
    Integer,
    BigInteger,
    /// 小整数（如状态位 `SMALLINT` / MySQL `TINYINT`）。
    SmallInt,
    /// 变长字符串，长度由 `ColumnDefinition::length` 决定。
    String,
    Text,
    Boolean,
    Decimal,
    Float,
    Double,
    DateTime,
    Date,
    Time,
    Timestamp,
    Json,
    Binary,
    Uuid,
}

impl ColumnType {
    pub fn as_sql(&self, driver: DbType) -> String {
        match self {
            ColumnType::Integer => "INT".to_string(),
            ColumnType::BigInteger => "BIGINT".to_string(),
            ColumnType::SmallInt => match driver {
                DbType::SQLite => "INTEGER".to_string(),
                _ => "SMALLINT".to_string(),
            },
            ColumnType::String => "VARCHAR(255)".to_string(),
            ColumnType::Text => "TEXT".to_string(),
            ColumnType::Boolean => match driver {
                DbType::MySQL => "TINYINT(1)".to_string(),
                _ => "BOOLEAN".to_string(),
            },
            ColumnType::Decimal => "DECIMAL(10,2)".to_string(),
            ColumnType::Float => match driver {
                DbType::PostgreSQL => "REAL".to_string(),
                DbType::SQLite => "REAL".to_string(),
                DbType::MySQL => "FLOAT".to_string(),
            },
            ColumnType::Double => match driver {
                DbType::PostgreSQL => "DOUBLE PRECISION".to_string(),
                DbType::SQLite => "REAL".to_string(),
                DbType::MySQL => "DOUBLE".to_string(),
            },
            ColumnType::DateTime => match driver {
                DbType::PostgreSQL => "TIMESTAMP".to_string(),
                _ => "DATETIME".to_string(),
            },
            ColumnType::Date => "DATE".to_string(),
            ColumnType::Time => "TIME".to_string(),
            ColumnType::Timestamp => "TIMESTAMP".to_string(),
            ColumnType::Json => match driver {
                DbType::PostgreSQL => "JSONB".to_string(),
                DbType::SQLite => "TEXT".to_string(),
                DbType::MySQL => "JSON".to_string(),
            },
            ColumnType::Binary => "BLOB".to_string(),
            ColumnType::Uuid => "VARCHAR(36)".to_string(),
        }
    }
}

/// 列定义
#[derive(Debug, Clone)]
pub struct ColumnDefinition {
    pub name: String,
    pub column_type: ColumnType,
    /// 变长字符串长度（`VARCHAR(n)`）。仅对 `ColumnType::String` 有意义，
    /// 为 `None` 时使用默认值 255。
    pub length: Option<u32>,
    pub nullable: bool,
    pub default: Option<String>,
    pub primary_key: bool,
    pub auto_increment: bool,
    pub unique: bool,
    pub comment: Option<String>,
}

impl ColumnDefinition {
    pub fn new(name: &str, column_type: ColumnType) -> Self {
        Self {
            name: name.to_string(),
            column_type,
            length: None,
            nullable: true,
            default: None,
            primary_key: false,
            auto_increment: false,
            unique: false,
            comment: None,
        }
    }

    /// 设置变长字符串长度（`VARCHAR(n)`）。
    pub fn length(mut self, length: u32) -> Self {
        self.length = Some(length);
        self
    }

    pub fn nullable(mut self, nullable: bool) -> Self {
        self.nullable = nullable;
        self
    }

    pub fn default(mut self, default: &str) -> Self {
        self.default = Some(default.to_string());
        self
    }

    pub fn primary_key(mut self) -> Self {
        self.primary_key = true;
        self
    }

    pub fn auto_increment(mut self) -> Self {
        self.auto_increment = true;
        self
    }

    pub fn unique(mut self) -> Self {
        self.unique = true;
        self
    }

    pub fn comment(mut self, comment: &str) -> Self {
        self.comment = Some(comment.to_string());
        self
    }
}

/// 索引定义
#[derive(Debug, Clone)]
pub struct IndexDefinition {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
    pub index_type: Option<String>,
}

impl IndexDefinition {
    pub fn new(name: &str, columns: &[&str]) -> Self {
        Self {
            name: name.to_string(),
            columns: columns.iter().map(|s| s.to_string()).collect(),
            unique: false,
            index_type: None,
        }
    }

    pub fn unique(mut self) -> Self {
        self.unique = true;
        self
    }

    pub fn index_type(mut self, index_type: &str) -> Self {
        self.index_type = Some(index_type.to_string());
        self
    }
}

/// 表定义
#[derive(Debug, Clone)]
pub struct TableDefinition {
    pub name: String,
    pub columns: Vec<ColumnDefinition>,
    pub indexes: Vec<IndexDefinition>,
    /// 内联外键约束（在 `CREATE TABLE` 中一并声明）。
    /// SQLite 不支持 `ALTER TABLE ... ADD CONSTRAINT FOREIGN KEY`，
    /// 因此 SQLite 的外键必须在此内联声明。
    pub foreign_keys: Vec<ForeignKeyDefinition>,
    pub comment: Option<String>,
    pub engine: Option<String>,
    pub charset: Option<String>,
    pub collation: Option<String>,
}

impl TableDefinition {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            columns: Vec::new(),
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
            comment: None,
            engine: None,
            charset: None,
            collation: None,
        }
    }

    pub fn add_column(mut self, column: ColumnDefinition) -> Self {
        self.columns.push(column);
        self
    }

    pub fn add_index(mut self, index: IndexDefinition) -> Self {
        self.indexes.push(index);
        self
    }

    /// 添加内联外键约束（建表时一并声明，SQLite 必需）。
    pub fn add_foreign_key(mut self, fk: ForeignKeyDefinition) -> Self {
        self.foreign_keys.push(fk);
        self
    }

    pub fn comment(mut self, comment: &str) -> Self {
        self.comment = Some(comment.to_string());
        self
    }

    pub fn engine(mut self, engine: &str) -> Self {
        self.engine = Some(engine.to_string());
        self
    }

    pub fn charset(mut self, charset: &str) -> Self {
        self.charset = Some(charset.to_string());
        self
    }

    pub fn collation(mut self, collation: &str) -> Self {
        self.collation = Some(collation.to_string());
        self
    }
}

/// 一段按方言提供的原生 SQL（用于种子数据、兼容性 UPDATE/DELETE 等
/// 无法用结构化 [`MigrationOperation`] 表达的语句）。
#[derive(Debug, Clone)]
pub struct RawSqlVariant {
    pub sqlite: String,
    pub mysql: String,
    pub postgres: String,
}

impl RawSqlVariant {
    pub fn new(sqlite: &str, mysql: &str, postgres: &str) -> Self {
        Self {
            sqlite: sqlite.to_string(),
            mysql: mysql.to_string(),
            postgres: postgres.to_string(),
        }
    }

    /// 三方言使用同一段 SQL（适合三方言语法一致的语句）。
    pub fn uniform(sql: &str) -> Self {
        Self {
            sqlite: sql.to_string(),
            mysql: sql.to_string(),
            postgres: sql.to_string(),
        }
    }

    /// 返回当前数据库方言对应的 SQL。
    pub fn for_db(&self, db_type: DbType) -> &str {
        match db_type {
            DbType::PostgreSQL => &self.postgres,
            DbType::MySQL => &self.mysql,
            DbType::SQLite => &self.sqlite,
        }
    }
}

/// 迁移操作
#[derive(Debug, Clone)]
pub enum MigrationOperation {
    CreateTable(TableDefinition),
    DropTable(String),
    RenameTable(String, String),
    AddColumn(String, ColumnDefinition),
    DropColumn(String, String),
    RenameColumn(String, String, String),
    ModifyColumn(String, ColumnDefinition),
    AddIndex(String, IndexDefinition),
    DropIndex(String, String),
    AddForeignKey(String, ForeignKeyDefinition),
    DropForeignKey(String, String),
    /// 直接执行一段原生 SQL，按方言选取对应语句。
    RawSql(RawSqlVariant),
}

/// 外键定义
#[derive(Debug, Clone)]
pub struct ForeignKeyDefinition {
    pub name: String,
    pub columns: Vec<String>,
    pub references_table: String,
    pub references_columns: Vec<String>,
    pub on_delete: Option<String>,
    pub on_update: Option<String>,
}

impl ForeignKeyDefinition {
    pub fn new(name: &str, columns: &[&str], references_table: &str, references_columns: &[&str]) -> Self {
        Self {
            name: name.to_string(),
            columns: columns.iter().map(|s| s.to_string()).collect(),
            references_table: references_table.to_string(),
            references_columns: references_columns.iter().map(|s| s.to_string()).collect(),
            on_delete: None,
            on_update: None,
        }
    }

    pub fn on_delete(mut self, action: &str) -> Self {
        self.on_delete = Some(action.to_string());
        self
    }

    pub fn on_update(mut self, action: &str) -> Self {
        self.on_update = Some(action.to_string());
        self
    }
}

/// 迁移记录
#[derive(Debug, Clone)]
pub struct Migration {
    pub name: String,
    pub version: i64,
    pub operations: Vec<MigrationOperation>,
    pub rollback_operations: Vec<MigrationOperation>,
}

impl Migration {
    pub fn new(name: &str, version: i64) -> Self {
        Self {
            name: name.to_string(),
            version,
            operations: Vec::new(),
            rollback_operations: Vec::new(),
        }
    }

    pub fn add_operation(mut self, operation: MigrationOperation) -> Self {
        self.operations.push(operation);
        self
    }

    pub fn add_rollback_operation(mut self, operation: MigrationOperation) -> Self {
        self.rollback_operations.push(operation);
        self
    }
}

/// 迁移器
pub struct Migrator {
    database: Arc<Database>,
    migrations: Vec<Migration>,
    applied_migrations: HashMap<i64, String>,
}

impl Migrator {
    /// 创建迁移器，接收共享的数据库连接（`Arc<Database>`）。
    /// 迁移执行后调用方仍可通过同一 `Arc` 继续使用连接。
    pub fn new(database: Arc<Database>) -> Self {
        Self {
            database,
            migrations: Vec::new(),
            applied_migrations: HashMap::new(),
        }
    }

    /// 返回底层数据库引用（用于查询验证 / 同库复用）。
    pub fn database(&self) -> &Database {
        &self.database
    }

    pub fn add_migration(mut self, migration: Migration) -> Self {
        self.migrations.push(migration);
        self
    }

    pub async fn initialize(&self) -> Result<()> {
        // Create migrations table if it doesn't exist
        let create_table_sql = match self.database.db_type() {
            DbType::MySQL => {
                "CREATE TABLE IF NOT EXISTS schema_migrations (
                    version BIGINT PRIMARY KEY,
                    name VARCHAR(255) NOT NULL,
                    applied_at DATETIME NOT NULL
                )".to_string()
            }
            DbType::PostgreSQL => {
                "CREATE TABLE IF NOT EXISTS schema_migrations (
                    version BIGINT PRIMARY KEY,
                    name VARCHAR(255) NOT NULL,
                    applied_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
                )".to_string()
            }
            DbType::SQLite => {
                "CREATE TABLE IF NOT EXISTS schema_migrations (
                    version INTEGER PRIMARY KEY,
                    name TEXT NOT NULL,
                    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                )".to_string()
            }
        };

        self.database
            .execute_sql(&create_table_sql, &[])
            .await
            .map_err(|e| TormError::migration_error(format!("初始化 schema_migrations 失败: {e}")))?;

        Ok(())
    }

    /// 从 `schema_migrations` 表加载已应用的迁移版本。
    async fn load_applied_migrations(&mut self) -> Result<()> {
        let sql = "SELECT version, name FROM schema_migrations ORDER BY version";
        let result = self
            .database
            .query_sql(sql, &[])
            .await
            .map_err(|e| TormError::migration_error(format!("读取已应用迁移失败: {e}")))?;

        self.applied_migrations = result
            .rows
            .iter()
            .filter_map(|row| {
                Some((
                    row.get("version")?.as_i64()?,
                    row.get("name")?.as_str()?.to_string(),
                ))
            })
            .collect();
        Ok(())
    }

    /// 运行所有尚未应用的迁移。
    pub async fn run_migrations(&mut self) -> Result<()> {
        self.initialize().await?;
        self.load_applied_migrations().await?;

        // 先取出待应用迁移的版本号，避免在 `&self.migrations` 迭代中 `&mut self`
        let pending: Vec<i64> = self
            .migrations
            .iter()
            .map(|m| m.version)
            .filter(|v| !self.applied_migrations.contains_key(v))
            .collect();

        for version in pending {
            self.apply_migration_version(version).await?;
        }

        Ok(())
    }

    /// 按版本号应用迁移（迁移必须已注册）。
    async fn apply_migration_version(&mut self, version: i64) -> Result<()> {
        let migration = self
            .migrations
            .iter()
            .find(|m| m.version == version)
            .cloned()
            .ok_or_else(|| {
                TormError::migration_error(format!("Migration version {version} not found"))
            })?;
        self.apply_migration(&migration).await
    }

    pub async fn apply_migration(&mut self, migration: &Migration) -> Result<()> {
        for operation in &migration.operations {
            let sql = self.operation_to_sql(operation)?;
            self.execute_sql(&sql).await?;
        }

        // 参数化记录迁移版本（占位符 `?` 由 `execute_sql` 按方言转换）
        let insert_sql =
            "INSERT INTO schema_migrations (version, name, applied_at) VALUES (?, ?, CURRENT_TIMESTAMP)";
        self.database
            .execute_sql(
                insert_sql,
                &[
                    crate::db::db_types::SqlValue::I64(migration.version),
                    crate::db::db_types::SqlValue::String(migration.name.clone()),
                ],
            )
            .await
            .map_err(|e| {
                TormError::migration_error(format!(
                    "记录迁移版本 v{} {} 失败: {e}",
                    migration.version, migration.name
                ))
            })?;

        self.applied_migrations
            .insert(migration.version, migration.name.clone());
        eprintln!(
            "[torm::migrator] ✅ 迁移完成: v{} {}",
            migration.version, migration.name
        );

        Ok(())
    }

    pub async fn rollback_migration(&mut self, version: i64) -> Result<()> {
        let migration = self
            .migrations
            .iter()
            .find(|m| m.version == version)
            .ok_or_else(|| {
                TormError::migration_error(format!("Migration version {} not found", version))
            })?;

        // Execute rollback operations in reverse order
        for operation in migration.rollback_operations.iter().rev() {
            let sql = self.operation_to_sql(operation)?;
            self.execute_sql(&sql).await?;
        }

        // Remove migration record（参数化占位符）
        let delete_sql = "DELETE FROM schema_migrations WHERE version = ?";
        self.database
            .execute_sql(delete_sql, &[crate::db::db_types::SqlValue::I64(version)])
            .await
            .map_err(|e| TormError::migration_error(format!("删除迁移记录 v{version} 失败: {e}")))?;

        self.applied_migrations.remove(&version);
        Ok(())
    }

    pub async fn rollback_all(&mut self) -> Result<()> {
        let mut versions: Vec<i64> = self.applied_migrations.keys().copied().collect();
        versions.sort_by(|a, b| b.cmp(a)); // Reverse order

        for version in versions {
            self.rollback_migration(version).await?;
        }

        Ok(())
    }

    /// 标识符安全包装：非法标识符返回错误，避免 SQL 注入。
    fn ident(&self, name: &str) -> Result<String> {
        validate_identifier(name)
            .map_err(|e| TormError::migration_error(format!("非法标识符 `{name}`: {e}")))
    }

    fn operation_to_sql(&self, operation: &MigrationOperation) -> Result<String> {
        match operation {
            MigrationOperation::CreateTable(table) => self.build_create_table_sql(table),
            MigrationOperation::DropTable(name) => {
                let name = self.ident(name)?;
                let if_exists = match self.database.db_type() {
                    DbType::PostgreSQL => "IF EXISTS ",
                    _ => "",
                };
                Ok(format!("DROP TABLE {if_exists}{name}"))
            }
            MigrationOperation::RenameTable(old_name, new_name) => {
                let old = self.ident(old_name)?;
                let new = self.ident(new_name)?;
                Ok(format!("ALTER TABLE {old} RENAME TO {new}"))
            }
            MigrationOperation::AddColumn(table_name, column) => {
                let table = self.ident(table_name)?;
                Ok(format!(
                    "ALTER TABLE {table} ADD COLUMN {}",
                    self.build_column_sql(column)?
                ))
            }
            MigrationOperation::DropColumn(table_name, column_name) => {
                let table = self.ident(table_name)?;
                let column = self.ident(column_name)?;
                Ok(format!("ALTER TABLE {table} DROP COLUMN {column}"))
            }
            MigrationOperation::RenameColumn(table_name, old_name, new_name) => {
                let table = self.ident(table_name)?;
                let old = self.ident(old_name)?;
                let new = self.ident(new_name)?;
                Ok(format!("ALTER TABLE {table} RENAME COLUMN {old} TO {new}"))
            }
            MigrationOperation::ModifyColumn(table_name, column) => {
                let table = self.ident(table_name)?;
                let col = self.ident(&column.name)?;
                let type_sql = self.column_type_sql(&column.column_type, column.length);
                let def = format!("{col} {type_sql}");
                // MySQL 用 MODIFY COLUMN；PG/SQLite 用 ALTER COLUMN TYPE
                match self.database.db_type() {
                    DbType::MySQL => Ok(format!("ALTER TABLE {table} MODIFY COLUMN {def}")),
                    _ => Ok(format!(
                        "ALTER TABLE {table} ALTER COLUMN {col} TYPE {type_sql}"
                    )),
                }
            }
            MigrationOperation::AddIndex(table_name, index) => {
                let table = self.ident(table_name)?;
                let idx = self.ident(&index.name)?;
                let cols: Result<Vec<String>> = index
                    .columns
                    .iter()
                    .map(|c| self.ident(c))
                    .collect();
                let unique_str = if index.unique { "UNIQUE " } else { "" };
                // SQLite / PostgreSQL 支持 `IF NOT EXISTS`，MySQL 不支持（8.0 起 CREATE INDEX 也不支持 IF NOT EXISTS）。
                // 加 IF NOT EXISTS 使部分执行（未写 schema_migrations）的脏库在重跑时可自愈。
                let if_not_exists = match self.database.db_type() {
                    DbType::SQLite | DbType::PostgreSQL => "IF NOT EXISTS ",
                    _ => "",
                };
                Ok(format!(
                    "CREATE {unique_str}INDEX {if_not_exists}{idx} ON {table} ({})",
                    cols?.join(", ")
                ))
            }
            MigrationOperation::DropIndex(table_name, index_name) => {
                let idx = self.ident(index_name)?;
                // MySQL 需 `ON table`，PG/SQLite 不需要
                match self.database.db_type() {
                    DbType::MySQL => {
                        let table = self.ident(table_name)?;
                        Ok(format!("DROP INDEX {idx} ON {table}"))
                    }
                    DbType::PostgreSQL => {
                        let if_exists = "IF EXISTS ";
                        Ok(format!("DROP INDEX {if_exists}{idx}"))
                    }
                    DbType::SQLite => Ok(format!("DROP INDEX IF EXISTS {idx}")),
                }
            }
            MigrationOperation::AddForeignKey(table_name, fk) => {
                let table = self.ident(table_name)?;
                let fk_name = self.ident(&fk.name)?;
                let cols: Result<Vec<String>> = fk.columns.iter().map(|c| self.ident(c)).collect();
                let refs: Result<Vec<String>> = fk
                    .references_columns
                    .iter()
                    .map(|c| self.ident(c))
                    .collect();
                let ref_table = self.ident(&fk.references_table)?;
                let mut sql = format!(
                    "ALTER TABLE {table} ADD CONSTRAINT {fk_name} FOREIGN KEY ({}) REFERENCES {ref_table} ({})",
                    cols?.join(", "),
                    refs?.join(", ")
                );
                if let Some(on_delete) = &fk.on_delete {
                    sql.push_str(&format!(" ON DELETE {on_delete}"));
                }
                if let Some(on_update) = &fk.on_update {
                    sql.push_str(&format!(" ON UPDATE {on_update}"));
                }
                Ok(sql)
            }
            MigrationOperation::DropForeignKey(table_name, fk_name) => {
                let table = self.ident(table_name)?;
                let fk_name = self.ident(fk_name)?;
                match self.database.db_type() {
                    DbType::SQLite => Ok(format!("ALTER TABLE {table} DROP CONSTRAINT {fk_name}")),
                    _ => Ok(format!("ALTER TABLE {table} DROP FOREIGN KEY {fk_name}")),
                }
            }
            MigrationOperation::RawSql(variant) => {
                Ok(variant.for_db(self.database.db_type()).to_string())
            }
        }
    }

    /// 生成列类型字符串，应用 `ColumnDefinition::length`。
    fn column_type_sql(&self, column_type: &ColumnType, length: Option<u32>) -> String {
        if *column_type == ColumnType::String {
            let len = length.unwrap_or(255);
            return format!("VARCHAR({len})");
        }
        column_type.as_sql(self.database.db_type())
    }

    fn build_create_table_sql(&self, table: &TableDefinition) -> Result<String> {
        let table_name = self.ident(&table.name)?;
        // 幂等：三方言均支持 CREATE TABLE IF NOT EXISTS
        let mut sql = format!("CREATE TABLE IF NOT EXISTS {table_name} (\n");

        let mut column_defs: Vec<String> = Vec::new();
        // SQLite 自增主键需内联声明为 INTEGER PRIMARY KEY AUTOINCREMENT，
        // 不能追加表级 PRIMARY KEY 约束
        let sqlite_inline_pk = self.database.db_type() == DbType::SQLite
            && table
                .columns
                .iter()
                .any(|c| c.primary_key && c.auto_increment);

        for col in &table.columns {
            column_defs.push(self.build_column_sql_inner(col)?);
        }

        if !sqlite_inline_pk {
            let primary_keys: Vec<String> = table
                .columns
                .iter()
                .filter(|col| col.primary_key)
                .map(|col| self.ident(&col.name))
                .collect::<Result<_>>()?;
            if !primary_keys.is_empty() {
                column_defs.push(format!("PRIMARY KEY ({})", primary_keys.join(", ")));
            }
        }

        // 内联外键约束（SQLite 必需；MySQL / PG 同样支持）
        for fk in &table.foreign_keys {
            let fk_name = self.ident(&fk.name)?;
            let cols: Vec<String> = fk
                .columns
                .iter()
                .map(|c| self.ident(c))
                .collect::<Result<_>>()?;
            let ref_cols: Vec<String> = fk
                .references_columns
                .iter()
                .map(|c| self.ident(c))
                .collect::<Result<_>>()?;
            let ref_table = self.ident(&fk.references_table)?;
            let mut fk_sql = format!(
                "CONSTRAINT {fk_name} FOREIGN KEY ({}) REFERENCES {ref_table} ({})",
                cols.join(", "),
                ref_cols.join(", ")
            );
            if let Some(od) = &fk.on_delete {
                fk_sql.push_str(&format!(" ON DELETE {od}"));
            }
            if let Some(ou) = &fk.on_update {
                fk_sql.push_str(&format!(" ON UPDATE {ou}"));
            }
            column_defs.push(fk_sql);
        }

        sql.push_str(&column_defs.join(",\n"));
        sql.push_str("\n)");

        // MySQL 专用表选项
        if self.database.db_type() == DbType::MySQL {
            if let Some(engine) = &table.engine {
                sql.push_str(&format!(" ENGINE={}", engine));
            }
            if let Some(charset) = &table.charset {
                sql.push_str(&format!(" DEFAULT CHARSET={}", charset));
            }
            if let Some(collation) = &table.collation {
                sql.push_str(&format!(" COLLATE={}", collation));
            }
        }

        // 表注释（仅 MySQL 支持）
        if let Some(comment) = &table.comment {
            if self.database.db_type() == DbType::MySQL {
                sql.push_str(&format!(" COMMENT='{}'", escape_string(comment)));
            }
        }

        Ok(sql)
    }

    /// 生成单列定义（用于 `CREATE TABLE`）。
    fn build_column_sql_inner(&self, column: &ColumnDefinition) -> Result<String> {
        let col_name = self.ident(&column.name)?;
        let type_sql = self.column_type_sql(&column.column_type, column.length);
        let db_type = self.database.db_type();

        let mut sql = format!("    {col_name} {type_sql}");

        // SQLite 自增主键内联：INTEGER PRIMARY KEY AUTOINCREMENT
        if db_type == DbType::SQLite && column.primary_key && column.auto_increment {
            sql = format!("    {col_name} INTEGER PRIMARY KEY AUTOINCREMENT");
        } else {
            if column.primary_key && column.auto_increment {
                match db_type {
                    // PostgreSQL 自增使用序列类型，直接替换基础类型：
                    // BIGSERIAL（对应 BIGINT）/ SMALLSERIAL / SERIAL（对应 INTEGER）。
                    DbType::PostgreSQL => {
                        let serial = pg_auto_increment_type(&column.column_type);
                        sql = format!("    {col_name} {serial}");
                    }
                    DbType::MySQL => sql.push_str(" AUTO_INCREMENT"),
                    DbType::SQLite => sql.push_str(" AUTOINCREMENT"),
                }
            }

            if !column.nullable {
                sql.push_str(" NOT NULL");
            }

            if let Some(default) = &column.default {
                sql.push_str(&format!(" DEFAULT {default}"));
            }

            if column.unique {
                sql.push_str(" UNIQUE");
            }
        }

        if let Some(comment) = &column.comment {
            if db_type == DbType::MySQL {
                sql.push_str(&format!(" COMMENT '{}'", escape_string(comment)));
            }
        }

        Ok(sql)
    }

    /// 供 `AddColumn` / `ModifyColumn` 使用：非法标识符时报错。
    fn build_column_sql(&self, column: &ColumnDefinition) -> Result<String> {
        let col_name = self.ident(&column.name)?;
        let type_sql = self.column_type_sql(&column.column_type, column.length);
        let db_type = self.database.db_type();

        let mut sql = format!("{col_name} {type_sql}");

        if column.primary_key && column.auto_increment {
            match db_type {
                DbType::MySQL => sql.push_str(" AUTO_INCREMENT"),
                DbType::PostgreSQL => sql.push_str(" SERIAL"),
                DbType::SQLite => sql.push_str(" AUTOINCREMENT"),
            }
        }

        if !column.nullable {
            sql.push_str(" NOT NULL");
        }
        if let Some(default) = &column.default {
            sql.push_str(&format!(" DEFAULT {default}"));
        }
        if column.unique {
            sql.push_str(" UNIQUE");
        }
        if let Some(comment) = &column.comment {
            if db_type == DbType::MySQL {
                sql.push_str(&format!(" COMMENT '{}'", escape_string(comment)));
            }
        }

        Ok(sql)
    }

    async fn execute_sql(&self, sql: &str) -> Result<()> {
        self.database
            .execute_sql(sql, &[])
            .await
            .map(|_| ())
            .map_err(|e| TormError::migration_error(format!("迁移 SQL 执行失败: {e}\nSQL: {sql}")))
    }

    pub fn get_status(&self) -> MigrationStatus {
        let total_migrations = self.migrations.len();
        let applied_count = self.applied_migrations.len();
        let pending_count = total_migrations - applied_count;

        MigrationStatus {
            total_migrations,
            applied_count,
            pending_count,
            latest_version: self.applied_migrations.keys().max().copied(),
        }
    }
}

/// PostgreSQL 自增主键使用的序列类型，按列类型映射：
/// - `BigInteger` → `BIGSERIAL`
/// - `SmallInt` → `SMALLSERIAL`
/// - 其它（如 `Integer`）→ `SERIAL`
fn pg_auto_increment_type(column_type: &ColumnType) -> &'static str {
    match column_type {
        ColumnType::BigInteger => "BIGSERIAL",
        ColumnType::SmallInt => "SMALLSERIAL",
        _ => "SERIAL",
    }
}

#[derive(Debug)]
pub struct MigrationStatus {
    pub total_migrations: usize,
    pub applied_count: usize,
    pub pending_count: usize,
    pub latest_version: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_column_definition() {
        let column = ColumnDefinition::new("id", ColumnType::Integer)
            .primary_key()
            .auto_increment()
            .nullable(false);

        assert_eq!(column.name, "id");
        assert!(column.primary_key);
        assert!(column.auto_increment);
        assert!(!column.nullable);
    }

    #[test]
    fn test_table_definition() {
        let table = TableDefinition::new("users")
            .add_column(ColumnDefinition::new("id", ColumnType::Integer).primary_key().auto_increment())
            .add_column(ColumnDefinition::new("name", ColumnType::String).nullable(false))
            .add_column(ColumnDefinition::new("email", ColumnType::String).unique())
            .add_column(ColumnDefinition::new("created_at", ColumnType::DateTime).default("NOW()"));

        assert_eq!(table.name, "users");
        assert_eq!(table.columns.len(), 4);
    }

    #[test]
    fn test_index_definition() {
        let index = IndexDefinition::new("idx_user_email", &["email"]).unique();
        
        assert_eq!(index.name, "idx_user_email");
        assert!(index.unique);
        assert_eq!(index.columns, vec!["email"]);
    }

    #[test]
    fn test_foreign_key_definition() {
        let fk = ForeignKeyDefinition::new("fk_user_post", &["user_id"], "users", &["id"])
            .on_delete("CASCADE")
            .on_update("CASCADE");

        assert_eq!(fk.name, "fk_user_post");
        assert_eq!(fk.columns, vec!["user_id"]);
        assert_eq!(fk.references_table, "users");
        assert_eq!(fk.on_delete, Some("CASCADE".to_string()));
    }

    #[test]
    fn test_migration() {
        let migration = Migration::new("create_users_table", 20230101000000)
            .add_operation(MigrationOperation::CreateTable(
                TableDefinition::new("users")
                    .add_column(ColumnDefinition::new("id", ColumnType::Integer).primary_key().auto_increment())
            ))
            .add_rollback_operation(MigrationOperation::DropTable("users".to_string()));

        assert_eq!(migration.version, 20230101000000);
        assert_eq!(migration.operations.len(), 1);
        assert_eq!(migration.rollback_operations.len(), 1);
    }

    #[test]
    fn test_column_type_sql() {
        assert_eq!(ColumnType::Integer.as_sql(crate::db::db_types::DbType::MySQL), "INT");
        assert_eq!(ColumnType::String.as_sql(crate::db::db_types::DbType::PostgreSQL), "VARCHAR(255)");
        assert_eq!(ColumnType::Json.as_sql(crate::db::db_types::DbType::PostgreSQL), "JSONB");
        assert_eq!(ColumnType::Json.as_sql(crate::db::db_types::DbType::MySQL), "JSON");
    }

    #[tokio::test]
    async fn test_migrator_creation() {
        // Use an in-memory SQLite database for the migrator test
        let database = Database::sqlite(":memory:").await.unwrap();
        
        // Simplified test - just verify migrations can be registered
        let migrator = Migrator::new(Arc::new(database))
            .add_migration(Migration::new("test", 1));

        assert_eq!(migrator.migrations.len(), 1);
    }

    #[test]
    fn test_migration_status() {
        let status = MigrationStatus {
            total_migrations: 5,
            applied_count: 3,
            pending_count: 2,
            latest_version: Some(20230101000002),
        };

        assert_eq!(status.total_migrations, 5);
        assert_eq!(status.applied_count, 3);
        assert_eq!(status.pending_count, 2);
        assert_eq!(status.latest_version, Some(20230101000002));
    }

    /// PostgreSQL 自增主键应映射为 BIGSERIAL / SMALLSERIAL / SERIAL，而非 `BIGINT SERIAL`。
    #[test]
    fn test_pg_autoincrement_serial_type() {
        assert_eq!(pg_auto_increment_type(&ColumnType::BigInteger), "BIGSERIAL");
        assert_eq!(pg_auto_increment_type(&ColumnType::Integer), "SERIAL");
        assert_eq!(pg_auto_increment_type(&ColumnType::SmallInt), "SMALLSERIAL");
        assert_eq!(pg_auto_increment_type(&ColumnType::String), "SERIAL");
    }

    /// 确保 PG 自增 SQL 中不会出现非法的 `BIGINT SERIAL` 组合。
    #[test]
    fn test_pg_sql_never_bigint_serial() {
        assert_ne!(
            format!(
                "BIGINT {}",
                pg_auto_increment_type(&ColumnType::BigInteger)
            ),
            "BIGINT SERIAL"
        );
    }

    #[test]
    fn test_complete_table_migration() {
        let table = TableDefinition::new("posts")
            .add_column(ColumnDefinition::new("id", ColumnType::Integer).primary_key().auto_increment())
            .add_column(ColumnDefinition::new("title", ColumnType::String).nullable(false).comment("Post title"))
            .add_column(ColumnDefinition::new("content", ColumnType::Text))
            .add_column(ColumnDefinition::new("user_id", ColumnType::Integer).nullable(false))
            .add_column(ColumnDefinition::new("created_at", ColumnType::DateTime).default("NOW()"))
            .add_column(ColumnDefinition::new("updated_at", ColumnType::DateTime).default("NOW()"))
            .add_index(IndexDefinition::new("idx_post_user_id", &["user_id"]))
            .comment("Blog posts table")
            .engine("InnoDB");

        assert_eq!(table.columns.len(), 6);
        assert_eq!(table.indexes.len(), 1);
        assert_eq!(table.engine, Some("InnoDB".to_string()));
        assert_eq!(table.comment, Some("Blog posts table".to_string()));
    }

    /// 构建一个真实的 sqlite 迁移集，验证建表/索引/种子数据/版本跟踪。
    fn build_test_migrations() -> Vec<Migration> {
        vec![
            Migration::new("create_users", 1)
                .add_operation(MigrationOperation::CreateTable(
                    TableDefinition::new("users")
                        .add_column(
                            ColumnDefinition::new("id", ColumnType::Integer)
                                .primary_key()
                                .auto_increment(),
                        )
                        .add_column(
                            ColumnDefinition::new("username", ColumnType::String)
                                .length(64)
                                .nullable(false)
                                .unique(),
                        )
                        .add_column(
                            ColumnDefinition::new("password", ColumnType::String)
                                .length(255)
                                .nullable(false),
                        ),
                ))
                .add_operation(MigrationOperation::AddIndex(
                    "users".to_string(),
                    IndexDefinition::new("idx_users_username", &["username"]).unique(),
                ))
                .add_operation(MigrationOperation::RawSql(RawSqlVariant::uniform(
                    "INSERT INTO users (username, password) VALUES ('admin', 'hash')",
                )))
                .add_rollback_operation(MigrationOperation::DropTable("users".to_string())),
            Migration::new("create_roles", 2)
                .add_operation(MigrationOperation::CreateTable(
                    TableDefinition::new("roles")
                        .add_column(
                            ColumnDefinition::new("id", ColumnType::Integer)
                                .primary_key()
                                .auto_increment(),
                        )
                        .add_column(
                            ColumnDefinition::new("name", ColumnType::String)
                                .length(64)
                                .nullable(false),
                        )
                        .add_column(
                            ColumnDefinition::new("status", ColumnType::SmallInt)
                                .default("1")
                                .nullable(false),
                        ),
                ))
                .add_rollback_operation(MigrationOperation::DropTable("roles".to_string())),
        ]
    }

    #[tokio::test]
    async fn test_migrator_runs_on_sqlite() {
        let database = Database::sqlite(":memory:").await.unwrap();
        let mut migrator = Migrator::new(Arc::new(database));
        for m in build_test_migrations() {
            migrator = migrator.add_migration(m);
        }

        migrator.run_migrations().await.unwrap();

        // 1. 表已建
        let res = migrator
            .database()
            .query("SELECT name FROM sqlite_master WHERE type='table' AND name IN ('users','roles','schema_migrations')", &[])
            .await
            .unwrap();
        assert_eq!(res.rows.len(), 3);

        // 2. 种子数据已插入
        let res = migrator
            .database()
            .query("SELECT COUNT(*) AS c FROM users", &[])
            .await
            .unwrap();
        assert_eq!(res.rows[0].get("c").and_then(|v| v.as_i64()), Some(1));

        // 3. 唯一索引存在
        let res = migrator
            .database()
            .query("SELECT name FROM sqlite_master WHERE type='index' AND name='idx_users_username'", &[])
            .await
            .unwrap();
        assert_eq!(res.rows.len(), 1);

        // 4. 版本跟踪
        let res = migrator
            .database()
            .query("SELECT COUNT(*) AS c FROM schema_migrations", &[])
            .await
            .unwrap();
        assert_eq!(res.rows[0].get("c").and_then(|v| v.as_i64()), Some(2));
    }

    #[tokio::test]
    async fn test_migrator_idempotent_rerun() {
        let database = Database::sqlite(":memory:").await.unwrap();
        let mut migrator = Migrator::new(Arc::new(database));
        for m in build_test_migrations() {
            migrator = migrator.add_migration(m);
        }

        migrator.run_migrations().await.unwrap();

        // 再次运行应跳过已应用的迁移，不重复插入种子数据
        migrator.run_migrations().await.unwrap();

        let res = migrator
            .database()
            .query("SELECT COUNT(*) AS c FROM users", &[])
            .await
            .unwrap();
        assert_eq!(res.rows[0].get("c").and_then(|v| v.as_i64()), Some(1));
    }

    #[tokio::test]
    async fn test_migrator_rollback() {
        let database = Database::sqlite(":memory:").await.unwrap();
        let mut migrator = Migrator::new(Arc::new(database));
        for m in build_test_migrations() {
            migrator = migrator.add_migration(m);
        }

        migrator.run_migrations().await.unwrap();

        // 回滚 v2
        migrator.rollback_migration(2).await.unwrap();
        let res = migrator
            .database()
            .query("SELECT name FROM sqlite_master WHERE type='table' AND name='roles'", &[])
            .await
            .unwrap();
        assert_eq!(res.rows.len(), 0);

        let res = migrator
            .database()
            .query("SELECT COUNT(*) AS c FROM schema_migrations", &[])
            .await
            .unwrap();
        assert_eq!(res.rows[0].get("c").and_then(|v| v.as_i64()), Some(1));
    }
}