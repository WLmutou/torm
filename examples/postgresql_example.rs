// TORM PostgreSQL Example
// PostgreSQL support is implemented via the native wire protocol:
//   ✅ Connection configuration and creation (real TCP connection)
//   ✅ Authentication: cleartext / MD5 / SCRAM-SHA-256
//   ✅ Simple query protocol + parameterized queries (Parse/Bind/Execute)
//   ✅ Result decoding (RowDescription/DataRow), transactions
//
// Prerequisites: a running PostgreSQL server, e.g.
//   psql -h localhost -p 5432 -U odoo -c "CREATE DATABASE tormdb;"

use torm::*;

const PG_HOST: &str = "localhost";
const PG_PORT: u16 = 5432;
const PG_DB: &str = "tormdb";
const PG_USER: &str = "odoo";
const PG_PASS: &str = "odoo";

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("🐘 TORM - Tokio ORM PostgreSQL Demo\n");

    // 1. Database connection example
    println!("📡 PostgreSQL connection example");
    println!("================================");
    demonstrate_database_connection().await?;
    println!();

    // 2. GORM-style model CRUD
    println!("🗄️  GORM-style model CRUD");
    println!("================================");
    demonstrate_crud().await?;
    println!();

    // 3. Transactions
    println!("🔒 PostgreSQL transactions");
    println!("================================");
    demonstrate_transactions().await?;
    println!();

    // 4. Simplified UUID generation
    println!("🔑 Simplified UUID generation");
    println!("================================");
    demonstrate_simplified_uuid()?;
    println!();

    // 5. Simplified error handling
    println!("⚠️  Simplified error handling");
    println!("================================");
    demonstrate_simplified_error()?;
    println!();

    // 6. Simplified LRU cache
    println!("💾 Simplified LRU cache");
    println!("================================");
    demonstrate_simplified_cache()?;
    println!();

    // 7. Simple connection pool
    println!("🏊 Simple connection pool");
    println!("================================");
    demonstrate_connection_pool().await?;
    println!();

    // 8. Query builder example
    println!("🔨 Query builder example");
    println!("================================");
    demonstrate_query_builder();
    println!();

    println!("🎉 All demos completed successfully!");
    println!();

    Ok(())
}

async fn demonstrate_database_connection() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("PostgreSQL connection (native wire protocol):");
    let dsn = Dsn::new(DBDriver::PostgreSQL, PG_DB)
        .with_host(PG_HOST)
        .with_port(PG_PORT)
        .with_username(PG_USER)
        .with_password(PG_PASS);
    println!("  DSN: {}", dsn.build());

    // Test actual PostgreSQL connection
    let db = Database::postgresql(PG_HOST, PG_PORT, PG_DB, PG_USER, PG_PASS).await?;
    println!("  ✅ Connected to PostgreSQL (native wire protocol)");
    println!("  DB type: {:?}", db.db_type());
    println!("  Connected: {}", db.is_connected());
    db.ping().await?;
    println!("  ✅ Ping successful");
    db.close().await?;

    println!();
    println!("Connection via ConnectionFactory:");
    let config = ConnectionConfig::postgresql(PG_HOST, PG_PORT, PG_DB, PG_USER, PG_PASS)
        .with_timeout(std::time::Duration::from_secs(30))
        .with_max_connections(10);
    let conn = ConnectionFactory::create_connection(config.clone()).await?;
    println!("  ✅ Connection created (factory)");
    println!("  DB type: {:?}", conn.db_type());
    conn.close().await?;

    println!();
    println!("Direct PostgresConnection:");
    let pg = PostgresConnection::new(&config).await?;
    println!("  ✅ PostgresConnection created");
    println!("  Connected: {}", pg.is_connected());
    pg.close().await?;

    Ok(())
}

async fn demonstrate_crud() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("GORM-style model CRUD (create / first / find / update / delete):");
    let db = Database::postgresql(PG_HOST, PG_PORT, PG_DB, PG_USER, PG_PASS).await?;

    // Schema setup (GORM AutoMigrate equivalent — drop + recreate for a clean demo)
    db.execute("DROP TABLE IF EXISTS users", &[]).await?;
    db.execute(
        "CREATE TABLE users (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            email TEXT,
            age INTEGER,
            status TEXT,
            created_at TIMESTAMPTZ,
            updated_at TIMESTAMPTZ
        )",
        &[],
    )
    .await?;
    println!("  ✅ Schema ready (users table recreated)");

    // Create — GORM `db.Create(&user)` equivalent
    let mut alice = User::new("Alice", "alice@example.com").with_age(25);
    db.create(&mut alice).await?;
    println!(
        "  ✅ Created: id={} name={} (created_at: {:?})",
        alice.id,
        alice.name,
        alice.created_at()
    );

    let mut bob = User::new("Bob", "bob@example.com").with_age(30);
    db.create(&mut bob).await?;
    println!("  ✅ Created: id={} name={}", bob.id, bob.name);

    // First — GORM `db.First(&user, id)` equivalent
    let found: Option<User> = db.first(&alice.id).await?;
    match &found {
        Some(u) => println!(
            "  ✅ First: id={} name={} age={:?} status={}",
            u.id, u.name, u.age, u.status
        ),
        None => println!("  ❌ First: not found"),
    }

    // Find all — GORM `db.Find(&users)` equivalent
    let users: Vec<User> = db.all().await?;
    println!("  ✅ Find: {} user(s)", users.len());
    for u in &users {
        println!(
            "    - {} <{}> age={:?} status={}",
            u.name, u.email, u.age, u.status
        );
    }

    // Update — GORM `db.Model(&user).Update("age", 29)` equivalent
    db.update(&mut alice, &[("age", 29)]).await?;
    let updated: User = db.first(&alice.id).await?.expect("alice exists");
    println!("  ✅ Updated: {} age now {:?}", updated.name, updated.age);

    // Delete — GORM `db.Delete(&user)` equivalent
    db.delete(&mut alice).await?;
    let gone: Option<User> = db.first(&alice.id).await?;
    println!(
        "  ✅ Deleted: {} exists after delete = {}",
        alice.name,
        gone.is_some()
    );

    db.close().await?;
    println!("  ✅ Database closed");

    Ok(())
}

async fn demonstrate_transactions() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("PostgreSQL transactions (BEGIN / COMMIT / ROLLBACK):");
    let db = Database::postgresql(PG_HOST, PG_PORT, PG_DB, PG_USER, PG_PASS).await?;

    // Commit
    let mut tx = db.begin_transaction().await?;
    tx.execute(
        "INSERT INTO users (id, name, age) VALUES ($1, $2, $3)",
        &[
            SimpleUuid::new_v4().to_string().into(),
            "Dave".into(),
            40.into(),
        ],
    )
    .await?;
    tx.execute(
        "INSERT INTO users (id, name, age) VALUES ($1, $2, $3)",
        &[
            SimpleUuid::new_v4().to_string().into(),
            "Frank".into(),
            35.into(),
        ],
    )
    .await?;
    tx.commit().await?;
    println!("  ✅ Transaction committed (2 users)");

    // Rollback
    let mut tx = db.begin_transaction().await?;
    tx.execute(
        "INSERT INTO users (id, name, age) VALUES ($1, $2, $3)",
        &[
            SimpleUuid::new_v4().to_string().into(),
            "Eve".into(),
            45.into(),
        ],
    )
    .await?;
    tx.rollback().await?;
    println!("  ✅ Transaction rolled back (Eve not saved)");

    let result = db.query("SELECT COUNT(*) AS count FROM users", &[]).await?;
    println!("  ✅ Final user count: {:?}", result.rows[0].get("count"));

    db.close().await?;
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
    println!(
        "  All unique: {}",
        uuid1 != uuid2 && uuid2 != uuid3 && uuid1 != uuid3
    );

    println!();
    println!("ID Generator:");
    let generator = IdGenerator::new();
    let id1 = generator.generate();
    let id2 = generator.generate();

    println!("  ID 1: {}", id1);
    println!("  ID 2: {}", id2);
    println!(
        "  ID 3: {}",
        generator.with_prefix("user_").generate()
    );

    println!();
    println!("Simple IDs:");
    let simple_gen = IdGenerator::new().with_simple_id();
    println!("  Simple ID: {}", simple_gen.generate());
    println!(
        "  Prefixed: {}",
        simple_gen.with_prefix("order_").generate()
    );

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
    println!(
        "  Utilization: {:.1}%",
        status.utilization_rate() * 100.0
    );

    // Test getting a connection
    println!();
    println!("  Getting a connection from pool:");
    match pool.get().await {
        Ok(conn) => {
            println!("    Got: {}", conn);
            let status = pool.status();
            println!(
                "    After get - idle: {}, active: {}",
                status.idle_connections, status.active_connections
            );
            pool.put(conn);
            let status = pool.status();
            println!(
                "    After put - idle: {}, active: {}",
                status.idle_connections, status.active_connections
            );
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

fn demonstrate_query_builder() {
    println!("Query builder examples:");
    println!("  (generates SQL text; note PostgreSQL executes with $1/$2-style parameters, see CRUD above)");

    // Basic query
    let (sql, bindings) = QueryBuilder::new(User::table_name())
        .where_eq("email", "john@example.com")
        .limit(1)
        .build();
    println!("  Basic query:");
    println!("    SQL: {}", sql);
    println!("    Bindings: {:?}", bindings);

    // Complex query
    let (sql, bindings) = QueryBuilder::new(User::table_name())
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
    let (sql, bindings) = QueryBuilder::new(User::table_name())
        .where_in("id", vec![1, 2, 3])
        .build();
    println!();
    println!("  IN query:");
    println!("    SQL: {}", sql);
    println!("    Bindings: {:?}", bindings);

    // BETWEEN query
    let (sql, bindings) = QueryBuilder::new(User::table_name())
        .where_between("age", 18, 65)
        .build();
    println!();
    println!("  BETWEEN query:");
    println!("    SQL: {}", sql);
    println!("    Bindings: {:?}", bindings);
}

// User model with simplified UUID
#[derive(Debug, Clone, Model)]
#[model(table_name = "users")]
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

