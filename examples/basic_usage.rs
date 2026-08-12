use torm::*;
use chrono::Utc;

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("🚀 TORM - Tokio ORM Demo (Simplified Dependencies)\n");

    // 1. Database connection example
    println!("📡 Database connection example");
    println!("========================");
    demonstrate_database_connection().await?;
    println!();

    // 2. Simplified UUID generation
    println!("🔑 Simplified UUID generation");
    println!("========================");
    demonstrate_simplified_uuid()?;
    println!();

    // 3. Simplified error handling
    println!("⚠️  Simplified error handling");
    println!("========================");
    demonstrate_simplified_error()?;
    println!();

    // 4. Simplified LRU cache
    println!("💾 Simplified LRU cache");
    println!("========================");
    demonstrate_simplified_cache()?;
    println!();

    // 5. Connection pool
    println!("🏊 Simple connection pool");
    println!("========================");
    demonstrate_connection_pool().await?;
    println!();

    // 6. Query builder example
    println!("🔨 Query builder example");
    println!("========================");
    demonstrate_query_builder();
    println!();

    // 7. Pure Rust SQL engine example
    println!("🗄️  Pure Rust SQL Engine");
    println!("========================");
    demonstrate_sql_engine().await?;
    println!();

    println!("🎉 All demos completed successfully!");
    println!();
    println!("📚 Simplified dependencies benefits:");
    println!("  ✅ Reduced external dependencies");
    println!("  ✅ Custom implementations for critical components");
    println!("  ✅ Pure Rust SQL engine (no rusqlite)");
    println!("  ✅ Better control over functionality and performance");
    println!("  ✅ Faster compilation with fewer dependencies");
    println!("  ✅ Smaller binary size");
    println!();
    println!("📦 Remaining dependencies:");
    println!("  • tokio - Async runtime (essential)");
    println!("  • serde/serde_json - Serialization (essential)");
    println!("  • chrono - Time handling (essential)");
    println!("  • uuid - UUID generation (essential)");

    Ok(())
}

async fn demonstrate_database_connection() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("SQLite connection:");
    let dsn = Dsn::new(DBDriver::SQLite, "demo.db");
    println!("  DSN: {}", dsn.build());
    
    // Test actual SQLite connection with pure Rust engine
    let db = Database::sqlite(":memory:").await?;
    println!("  ✅ Connected to in-memory SQLite (pure Rust engine)");
    println!("  DB type: {:?}", db.db_type());
    println!("  Connected: {}", db.is_connected());
    db.close().await?;
    
    println!();
    println!("MySQL connection example:");
    let mysql_dsn = Dsn::new(DBDriver::MySQL, "mydb")
        .with_host("localhost")
        .with_port(3306)
        .with_username("user")
        .with_password("password");
    println!("  DSN: {}", mysql_dsn.build());
    
    println!();
    println!("PostgreSQL connection example:");
    let pg_dsn = Dsn::new(DBDriver::PostgreSQL, "mydb")
        .with_host("localhost")
        .with_port(5432)
        .with_username("user")
        .with_password("password");
    println!("  DSN: {}", pg_dsn.build());

    Ok(())
}

fn demonstrate_simplified_uuid() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("Generating UUIDs:");
    
    // Generate multiple UUIDs
    let uuid1 = SimpleUuid::new_v4();
    let uuid2 = SimpleUuid::new_v4();
    let uuid3 = SimpleUuid::new_v4();
    
    println!("  UUID 1: {}", uuid1);
    println!("  UUID 2: {}", uuid2);
    println!("  UUID 3: {}", uuid3);
    println!("  All unique: {}", uuid1 != uuid2 && uuid2 != uuid3 && uuid1 != uuid3);
    
    println!();
    println!("ID Generator:");
    let generator = IdGenerator::new();
    let id1 = generator.generate();
    let id2 = generator.generate();
    
    println!("  ID 1: {}", id1);
    println!("  ID 2: {}", id2);
    println!("  ID 3: {}", generator.with_prefix("user_").generate());
    
    println!();
    println!("Simple IDs:");
    let simple_gen = IdGenerator::new().with_simple_id();
    println!("  Simple ID: {}", simple_gen.generate());
    println!("  Prefixed: {}", simple_gen.with_prefix("order_").generate());

    Ok(())
}

fn demonstrate_simplified_error() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("Error handling without thiserror:");
    
    // Create different error types
    let not_found = SimpleError::NotFound;
    println!("  NotFound: {}", not_found);
    
    let custom = SimpleError::custom("Something went wrong");
    println!("  Custom: {}", custom);
    
    let invalid_query = SimpleError::invalid_query("Invalid WHERE clause");
    println!("  InvalidQuery: {}", invalid_query);
    
    let connection_error = SimpleError::connection_error("Could not connect to database");
    println!("  ConnectionError: {}", connection_error);
    
    println!();
    println!("Using SimpleResult:");
    let success: SimpleResult<i32> = Ok(42);
    println!("  Success: {:?}", success);
    
    let failure: SimpleResult<i32> = Err(SimpleError::NotFound);
    println!("  Failure: {:?}", failure);

    Ok(())
}

fn demonstrate_simplified_cache() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("LRU cache:");
    
    let mut cache: SimpleLruCache<&str, &str> = SimpleLruCache::new(3);
    
    // Add items
    cache.put("key1", "value1");
    cache.put("key2", "value2");
    cache.put("key3", "value3");
    
    println!("  Initial size: {}", cache.len());
    println!("  key1: {:?}", cache.get(&"key1"));
    println!("  key2: {:?}", cache.get(&"key2"));
    println!("  key3: {:?}", cache.get(&"key3"));
    
    // Test LRU eviction
    println!();
    println!("  Adding key4 (should evict oldest):");
    cache.put("key4", "value4");
    println!("  key1: {:?}", cache.get(&"key1")); // Should be None
    println!("  key4: {:?}", cache.get(&"key4")); // Should be Some
    
    // Test capacity
    println!();
    println!("  Current capacity: {}", cache.capacity());
    println!("  Current size: {}", cache.len());
    
    // Resize
    cache.resize(2);
    println!("  After resize to 2:");
    println!("  New size: {}", cache.len());
    
    // Cleanup
    cache.clear();
    println!("  After clear: {}", cache.is_empty());

    Ok(())
}

async fn demonstrate_connection_pool() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("Simple connection pool implementation:");
    
    // Create a pool with pre-created connections
    let connections = vec![1, 2, 3, 4, 5];
    let pool = SimplePool::new(connections);
    let status = pool.status();
    
    println!("  Total connections: {}", status.total_connections);
    println!("  Idle connections: {}", status.idle_connections);
    println!("  Active connections: {}", status.active_connections);
    println!("  Utilization: {:.1}%", status.utilization_rate() * 100.0);
    
    // Test getting a connection
    println!();
    println!("  Getting a connection from pool:");
    match pool.get().await {
        Ok(conn) => {
            println!("    Got: {}", conn);
            let status = pool.status();
            println!("    After get - idle: {}, active: {}", status.idle_connections, status.active_connections);
            pool.put(conn);
            let status = pool.status();
            println!("    After put - idle: {}, active: {}", status.idle_connections, status.active_connections);
        }
        Err(e) => println!("    Error: {}", e),
    }
    
    println!();
    println!("Pool features:");
    println!("  • Connection reuse");
    println!("  • Timeout handling");
    println!("  • No external deadpool dependency");

    Ok(())
}

async fn demonstrate_sql_engine() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("Pure Rust SQL engine (no rusqlite) + typed model:");
    
    let db = Database::sqlite(":memory:").await?;
    
    // 依据模型自动建表（零 SqlValue）
    db.auto_migrate::<Product>().await?;
    println!("  ✅ Created products table from model schema");
    
    // 通过模型 create 插入
    let mut products = vec![
        Product { id: 0, name: "Apple".to_string(), price: 5 },
        Product { id: 0, name: "Banana".to_string(), price: 3 },
        Product { id: 0, name: "Cherry".to_string(), price: 9 },
    ];
    for p in &mut products {
        db.create(p).await?;
    }
    println!("  ✅ Inserted {} products", products.len());
    
    // 查询并映射回类型
    let all: Vec<Product> = db.all::<Product>().await?;
    println!("  ✅ Query returned {} rows", all.len());
    for p in &all {
        println!("    - {} price={}", p.name, p.price);
    }
    
    // 更新（返回影响行数）
    let affected = db.update(&mut products[0], &[("price", 6)]).await?;
    println!("  ✅ Updated {} row(s)", affected);
    
    // 计数
    let count = db.all::<Product>().await?.len();
    println!("  ✅ Count = {}", count);
    
    // 删除（使用已回填主键的模型实例）
    let affected = db.delete(&mut products[2]).await?;
    println!("  ✅ Deleted {} row(s)", affected);
    
    db.close().await?;
    println!("  ✅ Database closed");

    Ok(())
}

fn demonstrate_query_builder() {
    println!("Query builder examples:");
    
    // Basic query
    let (sql, bindings) = QueryBuilder::new("users")
        .where_eq("email", "john@example.com")
        .limit(1)
        .build();
    println!("  Basic query:");
    println!("    SQL: {}", sql);
    println!("    Bindings: {:?}", bindings);

    // Complex query
    let (sql, bindings) = QueryBuilder::new("users")
        .where_eq("status", "active")
        .where_gt("age", 18)
        .where_like("name", "John%")
        .order_by("created_at", "DESC")
        .limit(10)
        .build();
    println!();
    println!("  Complex query:");
    println!("    SQL: {}", sql);
    println!("    Bindings: {:?}", bindings);

    // IN query
    let (sql, bindings) = QueryBuilder::new("users")
        .where_in("id", vec![1, 2, 3])
        .build();
    println!();
    println!("  IN query:");
    println!("    SQL: {}", sql);
    println!("    Bindings: {:?}", bindings);

    // BETWEEN query
    let (sql, bindings) = QueryBuilder::new("users")
        .where_between("age", 18, 65)
        .build();
    println!();
    println!("  BETWEEN query:");
    println!("    SQL: {}", sql);
    println!("    Bindings: {:?}", bindings);
}

/// Product 模型：使用 `#[derive(Model)]`，由宏自动生成 schema 与字段映射。
#[derive(Debug, Clone, Model)]
#[model(table_name = "products")]
pub struct Product {
    pub id: i64,
    pub name: String,
    pub price: i64,
}

// User model with simplified UUID
#[derive(Debug, Clone)]
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
        let generator = IdGenerator::new().with_prefix("user_");
        Self {
            id: generator.generate(),
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