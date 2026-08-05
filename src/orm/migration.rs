use crate::db::error::{Result, TormError};
use crate::db::database::Database;
use std::collections::HashMap;

/// 数据类型枚举
#[derive(Debug, Clone, Copy)]
pub enum ColumnType {
    Integer,
    BigInteger,
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
    pub fn as_sql(&self, driver: crate::db::db_types::DbType) -> String {
        match (self, driver) {
            (ColumnType::Integer, _) => "INT".to_string(),
            (ColumnType::BigInteger, _) => "BIGINT".to_string(),
            (ColumnType::String, _) => "VARCHAR(255)".to_string(),
            (ColumnType::Text, _) => "TEXT".to_string(),
            (ColumnType::Boolean, _) => "BOOLEAN".to_string(),
            (ColumnType::Decimal, _) => "DECIMAL(10,2)".to_string(),
            (ColumnType::Float, _) => "FLOAT".to_string(),
            (ColumnType::Double, _) => "DOUBLE".to_string(),
            (ColumnType::DateTime, _) => "DATETIME".to_string(),
            (ColumnType::Date, _) => "DATE".to_string(),
            (ColumnType::Time, _) => "TIME".to_string(),
            (ColumnType::Timestamp, _) => "TIMESTAMP".to_string(),
            (ColumnType::Json, crate::db::db_types::DbType::PostgreSQL) => "JSONB".to_string(),
            (ColumnType::Json, _) => "JSON".to_string(),
            (ColumnType::Binary, _) => "BLOB".to_string(),
            (ColumnType::Uuid, _) => "VARCHAR(36)".to_string(),
        }
    }
}

/// 列定义
#[derive(Debug, Clone)]
pub struct ColumnDefinition {
    pub name: String,
    pub column_type: ColumnType,
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
            nullable: true,
            default: None,
            primary_key: false,
            auto_increment: false,
            unique: false,
            comment: None,
        }
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
    database: Database,
    migrations: Vec<Migration>,
    applied_migrations: HashMap<i64, String>,
}

impl Migrator {
    pub fn new(database: Database) -> Self {
        Self {
            database,
            migrations: Vec::new(),
            applied_migrations: HashMap::new(),
        }
    }

    pub fn add_migration(mut self, migration: Migration) -> Self {
        self.migrations.push(migration);
        self
    }

    pub async fn initialize(&self) -> Result<()> {
        // Create migrations table if it doesn't exist
        let create_table_sql = match self.database.db_type() {
            crate::db::db_types::DbType::MySQL => {
                "CREATE TABLE IF NOT EXISTS schema_migrations (
                    version BIGINT PRIMARY KEY,
                    name VARCHAR(255) NOT NULL,
                    applied_at DATETIME NOT NULL
                )".to_string()
            }
            crate::db::db_types::DbType::PostgreSQL => {
                "CREATE TABLE IF NOT EXISTS schema_migrations (
                    version BIGINT PRIMARY KEY,
                    name VARCHAR(255) NOT NULL,
                    applied_at TIMESTAMP NOT NULL
                )".to_string()
            }
            crate::db::db_types::DbType::SQLite => {
                "CREATE TABLE IF NOT EXISTS schema_migrations (
                    version INTEGER PRIMARY KEY,
                    name TEXT NOT NULL,
                    applied_at TEXT NOT NULL
                )".to_string()
            }
        };

        self.execute_sql(&create_table_sql).await?;
        
        // Simplified migration loading - would need actual database queries in production
        // self.load_applied_migrations().await?;
        
        Ok(())
    }

    #[allow(dead_code)]
    async fn load_applied_migrations(&mut self) -> Result<()> {
        let _select_sql = "SELECT version, name FROM schema_migrations ORDER BY version";
        
        // This would need actual query execution - simplified for now
        // In a real implementation, this would query the database and populate self.applied_migrations
        
        Ok(())
    }

    pub async fn run_migrations(&self) -> Result<()> {
        self.initialize().await?;

        for migration in &self.migrations {
            if !self.applied_migrations.contains_key(&migration.version) {
                self.apply_migration(migration).await?;
            }
        }

        Ok(())
    }

    pub async fn apply_migration(&self, migration: &Migration) -> Result<()> {
        // Begin transaction
        // For each operation, execute the SQL
        // Record the migration in schema_migrations
        // Commit transaction

        for operation in &migration.operations {
            let sql = self.operation_to_sql(operation)?;
            self.execute_sql(&sql).await?;
        }

        // Record migration
        let insert_sql = format!(
            "INSERT INTO schema_migrations (version, name, applied_at) VALUES ({}, '{}', NOW())",
            migration.version, migration.name
        );
        
        self.execute_sql(&insert_sql).await?;

        Ok(())
    }

    pub async fn rollback_migration(&self, version: i64) -> Result<()> {
        let migration = self.migrations.iter()
            .find(|m| m.version == version)
            .ok_or_else(|| TormError::MigrationError(format!("Migration version {} not found", version)))?;

        // Execute rollback operations in reverse order
        for operation in migration.rollback_operations.iter().rev() {
            let sql = self.operation_to_sql(operation)?;
            self.execute_sql(&sql).await?;
        }

        // Remove migration record
        let delete_sql = format!("DELETE FROM schema_migrations WHERE version = {}", version);
        self.execute_sql(&delete_sql).await?;

        Ok(())
    }

    pub async fn rollback_all(&self) -> Result<()> {
        let mut versions: Vec<i64> = self.applied_migrations.keys().copied().collect();
        versions.sort_by(|a, b| b.cmp(a)); // Reverse order

        for version in versions {
            self.rollback_migration(version).await?;
        }

        Ok(())
    }

    fn operation_to_sql(&self, operation: &MigrationOperation) -> Result<String> {
        match operation {
            MigrationOperation::CreateTable(table) => {
                Ok(self.build_create_table_sql(table))
            }
            MigrationOperation::DropTable(name) => {
                Ok(format!("DROP TABLE {}", name))
            }
            MigrationOperation::RenameTable(old_name, new_name) => {
                Ok(format!("ALTER TABLE {} RENAME TO {}", old_name, new_name))
            }
            MigrationOperation::AddColumn(table_name, column) => {
                Ok(format!("ALTER TABLE {} ADD COLUMN {}", table_name, self.build_column_sql(column)))
            }
            MigrationOperation::DropColumn(table_name, column_name) => {
                Ok(format!("ALTER TABLE {} DROP COLUMN {}", table_name, column_name))
            }
            MigrationOperation::RenameColumn(table_name, old_name, new_name) => {
                Ok(format!("ALTER TABLE {} RENAME COLUMN {} TO {}", table_name, old_name, new_name))
            }
            MigrationOperation::ModifyColumn(table_name, column) => {
                Ok(format!("ALTER TABLE {} MODIFY COLUMN {}", table_name, self.build_column_sql(column)))
            }
            MigrationOperation::AddIndex(table_name, index) => {
                let unique_str = if index.unique { "UNIQUE " } else { "" };
                Ok(format!("CREATE {}INDEX {} ON {} ({})", 
                    unique_str, index.name, table_name, index.columns.join(", ")))
            }
            MigrationOperation::DropIndex(table_name, index_name) => {
                Ok(format!("DROP INDEX {} ON {}", index_name, table_name))
            }
            MigrationOperation::AddForeignKey(table_name, fk) => {
                Ok(format!("ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {}({})",
                    table_name, fk.name, fk.columns.join(", "), fk.references_table, fk.references_columns.join(", ")))
            }
            MigrationOperation::DropForeignKey(table_name, fk_name) => {
                Ok(format!("ALTER TABLE {} DROP FOREIGN KEY {}", table_name, fk_name))
            }
        }
    }

    fn build_create_table_sql(&self, table: &TableDefinition) -> String {
        let mut sql = format!("CREATE TABLE {} (\n", table.name);

        let mut column_defs: Vec<String> = table.columns.iter()
            .map(|col| self.build_column_sql(col))
            .collect();

        // Add primary key constraint
        let primary_keys: Vec<String> = table.columns.iter()
            .filter(|col| col.primary_key)
            .map(|col| col.name.clone())
            .collect();

        if !primary_keys.is_empty() {
            column_defs.push(format!("PRIMARY KEY ({})", primary_keys.join(", ")));
        }

        sql.push_str(&column_defs.join(",\n"));
        sql.push_str("\n)");

        // Add MySQL-specific options
        if let Some(engine) = &table.engine {
            sql.push_str(&format!(" ENGINE={}", engine));
        }
        if let Some(charset) = &table.charset {
            sql.push_str(&format!(" CHARSET={}", charset));
        }
        if let Some(collation) = &table.collation {
            sql.push_str(&format!(" COLLATE={}", collation));
        }

        // Add comment
        if let Some(comment) = &table.comment {
            sql.push_str(&format!(" COMMENT='{}'", comment));
        }

        sql
    }

    fn build_column_sql(&self, column: &ColumnDefinition) -> String {
        let mut sql = String::new();
        
        sql.push_str(&format!("    {} {}", column.name, column.column_type.as_sql(self.database.db_type())));

        if column.primary_key && column.auto_increment {
            match self.database.db_type() {
                crate::db::db_types::DbType::MySQL => sql.push_str(" AUTO_INCREMENT"),
                crate::db::db_types::DbType::PostgreSQL => sql.push_str(" SERIAL"),
                crate::db::db_types::DbType::SQLite => sql.push_str(" AUTOINCREMENT"),
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

        if let Some(comment) = &column.comment {
            match self.database.db_type() {
                crate::db::db_types::DbType::MySQL => sql.push_str(&format!(" COMMENT '{}'", comment)),
                _ => {} // Other databases don't support inline column comments
            }
        }

        sql
    }

    async fn execute_sql(&self, sql: &str) -> Result<()> {
        // This would need actual SQL execution through the database
        // Simplified for now - in a real implementation, this would use the database connection
        println!("Executing SQL: {}", sql);
        Ok(())
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
        let migrator = Migrator::new(database)
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
}