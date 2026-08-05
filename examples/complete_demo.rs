use torm::{Database, DBDriver, Dsn, Model, Query, QueryBuilder, SqlValue};
use chrono::Utc;
use serde::{Deserialize, Serialize};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 TORM - Tokio ORM 示例演示\n");

    // 1. 数据库连接示例
    println!("📡 数据库连接示例");
    println!("========================");
    demonstrate_database_connection().await?;
    println!();

    // 2. 查询构建器示例
    println!("🔨 查询构建器示例");
    println!("========================");
    demonstrate_query_builder();
    println!();

    // 3. 高级查询示例
    println!("🔍 高级查询示例");
    println!("========================");
    demonstrate_advanced_queries();
    println!();

    // 4. CRUD 操作示例
    println!("💾 CRUD 操作示例");
    println!("========================");
    demonstrate_crud_operations().await?;
    println!();

    // 5. 事务示例
    println!("🔄 事务操作示例");
    println!("========================");
    demonstrate_transactions();
    println!();

    println!("🎉 所有示例演示完成!");
    println!();
    println!("📚 TORM 特性总结:");
    println!("  ✅ 多数据库支持 (MySQL, PostgreSQL, SQLite)");
    println!("  ✅ 异步/await 操作");
    println!("  ✅ 流畅的查询构建器API");
    println!("  ✅ 模型 trait 和自动时间戳管理");
    println!("  ✅ 事务支持");
    println!("  ✅ 复杂查询和分页");
    println!("  ✅ 生命周期钩子");

    Ok(())
}

// 示例模型结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub name: String,
    pub email: String,
    pub age: Option<i32>,
    pub status: String,
    pub timestamps: torm::orm::model::Timestamps,
}

impl User {
    pub fn new(name: &str, email: &str) -> Self {
        Self {
            id: String::new(),
            name: name.to_string(),
            email: email.to_string(),
            age: None,
            status: "active".to_string(),
            timestamps: torm::orm::model::Timestamps::new(),
        }
    }

    pub fn with_age(mut self, age: i32) -> Self {
        self.age = Some(age);
        self
    }

    pub fn with_status(mut self, status: &str) -> Self {
        self.status = status.to_string();
        self
    }

    pub fn with_id(mut self, id: String) -> Self {
        self.id = id;
        self
    }
}

#[async_trait::async_trait]
impl Model for User {
    fn table_name() -> &'static str {
        "users"
    }

    fn id(&self) -> Option<String> {
        if self.id.is_empty() {
            None
        } else {
            Some(self.id.clone())
        }
    }

    fn set_id(&mut self, id: String) {
        self.id = id;
    }

    fn created_at(&self) -> Option<chrono::DateTime<Utc>> {
        self.timestamps.created_at
    }

    fn updated_at(&self) -> Option<chrono::DateTime<Utc>> {
        self.timestamps.updated_at
    }

    fn deleted_at(&self) -> Option<chrono::DateTime<Utc>> {
        self.timestamps.deleted_at
    }

    fn set_created_at(&mut self, timestamp: chrono::DateTime<Utc>) {
        self.timestamps.created_at = Some(timestamp);
    }

    fn set_updated_at(&mut self, timestamp: chrono::DateTime<Utc>) {
        self.timestamps.updated_at = Some(timestamp);
    }

    fn set_deleted_at(&mut self, timestamp: Option<chrono::DateTime<Utc>>) {
        self.timestamps.deleted_at = timestamp;
    }
}

async fn demonstrate_database_connection() -> Result<(), Box<dyn std::error::Error>> {
    println!("SQLite 连接:");
    let dsn = Dsn::new(DBDriver::SQLite, "demo.db");
    println!("  DSN: {}", dsn.build());
    
    let database = Database::sqlite(":memory:").await?;
    println!("  ✅ 连接成功");
    println!("  驱动: {:?}", database.db_type());
    
    // 测试连接
    database.ping().await?;
    println!("  ✅ Ping 成功");
    database.close().await?;
    
    println!();
    println!("MySQL 连接示例:");
    let mysql_dsn = Dsn::new(DBDriver::MySQL, "mydb")
        .with_host("localhost")
        .with_port(3306)
        .with_username("user")
        .with_password("password");
    println!("  DSN: {}", mysql_dsn.build());
    
    println!();
    println!("PostgreSQL 连接示例:");
    let pg_dsn = Dsn::new(DBDriver::PostgreSQL, "mydb")
        .with_host("localhost")
        .with_port(5432)
        .with_username("user")
        .with_password("password");
    println!("  DSN: {}", pg_dsn.build());

    Ok(())
}

fn demonstrate_query_builder() {
    // 基本查询
    let (sql, bindings) = QueryBuilder::new("users")
        .where_eq("email", "john@example.com")
        .limit(1)
        .build();
    println!("基本查询:");
    println!("  SQL: {}", sql);
    println!("  Bindings: {:?}", bindings);

    // 复杂查询
    let (sql, bindings) = QueryBuilder::new("users")
        .where_eq("status", "active")
        .where_gt("age", "18")
        .where_like("name", "John%")
        .order_by("created_at", "DESC")
        .limit(10)
        .offset(5)
        .build();
    println!();
    println!("复杂查询:");
    println!("  SQL: {}", sql);
    println!("  Bindings: {:?}", bindings);

    // IN 查询
    let (sql, bindings) = QueryBuilder::new("users")
        .where_in("id", vec![SqlValue::I32(1), SqlValue::I32(2), SqlValue::I32(3)])
        .build();
    println!();
    println!("IN 查询:");
    println!("  SQL: {}", sql);
    println!("  Bindings: {:?}", bindings);

    // BETWEEN 查询
    let (sql, bindings) = QueryBuilder::new("users")
        .where_between("age", 18, 65)
        .build();
    println!();
    println!("BETWEEN 查询:");
    println!("  SQL: {}", sql);
    println!("  Bindings: {:?}", bindings);
}

fn demonstrate_advanced_queries() {
    // 分页查询
    let (sql, bindings) = Query::new("users").paginate(2, 10).build();
    println!("分页查询 (第2页，每页10条):");
    println!("  SQL: {}", sql);
    println!("  Bindings: {:?}", bindings);

    // 多条件查询
    let (sql, bindings) = Query::new("users")
        .where_eq("status", "active")
        .where_gt("age", "18")
        .where_like("name", "John%")
        .where_null("deleted_at")
        .order_by_desc("created_at")
        .limit(20)
        .build();
    println!();
    println!("多条件查询:");
    println!("  SQL: {}", sql);
    println!("  Bindings: {:?}", bindings);

    // 计数查询
    let (sql, bindings) = Query::new("users")
        .where_eq("status", "active")
        .where_null("deleted_at")
        .count();
    println!();
    println!("计数查询:");
    println!("  SQL: {}", sql);
    println!("  Bindings: {:?}", bindings);

    // 更新查询
    let mut updates = std::collections::HashMap::new();
    updates.insert("name".to_string(), SqlValue::String("John Doe Updated".to_string()));
    updates.insert("age".to_string(), SqlValue::I32(26));
    updates.insert("status".to_string(), SqlValue::String("verified".to_string()));

    let (sql, bindings) = Query::new("users")
        .where_eq("id", "123")
        .update(&updates);
    println!();
    println!("更新查询:");
    println!("  SQL: {}", sql);
    println!("  Bindings: {:?}", bindings);

    // 删除查询
    let (sql, bindings) = Query::new("users")
        .where_eq("id", "123")
        .delete();
    println!();
    println!("删除查询:");
    println!("  SQL: {}", sql);
    println!("  Bindings: {:?}", bindings);
}

async fn demonstrate_crud_operations() -> Result<(), Box<dyn std::error::Error>> {
    println!("创建用户:");
    let mut user = User::new("张三", "zhangsan@example.com")
        .with_age(25)
        .with_status("active");
    
    // 调用生命周期钩子（同步版本）
    user.before_save()?;
    println!("  用户: {:?}", user);
    println!("  ID: {:?}", user.id());
    println!("  创建时间: {:?}", user.created_at());
    
    // 更新用户
    println!();
    println!("更新用户:");
    let mut updated_user = user.with_age(26).with_status("verified");
    updated_user.before_update()?;
    println!("  更新后的用户: {:?}", updated_user);
    
    // 软删除示例
    println!();
    println!("软删除用户:");
    let mut deleted_user = updated_user;
    let now = Utc::now();
    deleted_user.set_deleted_at(Some(now));
    println!("  是否已删除: {}", deleted_user.is_deleted());
    println!("  删除时间: {:?}", deleted_user.deleted_at());

    Ok(())
}

fn demonstrate_transactions() {
    println!("TORM 事务支持:");
    println!("  ✅ begin_transaction() - 开始事务");
    println!("  ✅ commit() - 提交事务");
    println!("  ✅ rollback() - 回滚事务");
    println!("  ✅ transaction() - 自动事务管理");
    println!();
    println!("示例:");
    let example_code = r#"
// 自动事务示例
let result = database.transaction(|tx| async move {
    // 执行多个操作
    let result1 = some_operation(&tx).await?;
    let result2 = another_operation(&tx).await?;
    
    // 如果所有操作成功，自动提交
    Ok((result1, result2))
}).await?;

// 如果任何操作失败，自动回滚
"#;
    println!("{}", example_code);
}