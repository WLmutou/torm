# TORM - Tokio ORM

TORM is a Rust ORM (Object-Relational Mapping) library built on the Tokio async runtime, providing GORM-like functionality with a layered module design (Database / ORM / Utils / Monitoring).

## 🎯 Key Features

- ✅ **Standard SQLite Support** - Built on rusqlite, generates standard SQLite file format (readable by sqlite3 and other SQLite tools)
- ✅ **Pure Rust Storage Engine** - Built-in zero-dependency in-memory storage engine (StorageEngine)
- ✅ **MySQL/PostgreSQL Framework** - Protocol framework support
- ✅ **Async/await Support** - Fully based on the Tokio async runtime
- ✅ **Multi-Database Support** - MySQL, PostgreSQL, SQLite
- ✅ **Fluent Query Builder** - Clean and intuitive query API
- ✅ **Advanced Queries** - JOIN, GROUP BY, HAVING, aggregate functions
- ✅ **Model Trait** - Automatic management of created_at, updated_at timestamps
- ✅ **Transaction Support** - Create, commit, and rollback transactions
- ✅ **Connection Pooling** - Pools for SQLite/MySQL/PostgreSQL
- ✅ **Logging & Performance Monitoring** - Built-in logging system and performance stats

## 📦 Dependencies

```toml
[dependencies]
tokio = "1.53"              # Async runtime
rusqlite = { version = "0.30", features = ["bundled"] }  # SQLite (standard file format)
uuid = "1.0"                # UUID generation
serde = "1.0"               # Serialization
serde_json = "1.0"          # JSON support
chrono = "0.4"              # Time handling
async-trait = "0.1"         # Async traits
thiserror = "1.0"           # Error derivation
```

### Database Layer Implementation

| Feature | Implementation | Status |
|---------|---------------|--------|
| SQLite | rusqlite (standard file format) | ✅ Complete |
| In-memory engine | Pure Rust StorageEngine | ✅ Complete |
| MySQL | Custom protocol framework | ⚠️ Framework |
| PostgreSQL | Custom protocol framework | ⚠️ Framework |
| Type safety | Custom SqlValue | ✅ Complete |
| Transactions | Custom implementation | ✅ Complete |

## 🏗 Module Structure

```
src/
├── lib.rs              # Module declarations and exports
├── db/                 # Database layer
│   ├── db_types.rs     # SQL type system (SqlValue, Row, QueryResult)
│   ├── database.rs     # Connection abstraction, transactions, factory, Database
│   ├── driver.rs       # DBDriver, Dsn
│   ├── error.rs        # TormError
│   ├── storage.rs      # Pure Rust in-memory storage engine
│   ├── sqlite.rs       # SQLite implementation (rusqlite backend)
│   ├── mysql.rs        # MySQL protocol framework
│   ├── postgresql.rs   # PostgreSQL protocol framework
│   └── pool.rs         # Connection pools
├── orm/                # ORM layer
│   ├── model.rs        # Model trait
│   ├── query.rs        # Query/QueryBuilder
│   ├── advanced_query.rs # Advanced queries (JOIN/GROUP BY/aggregates)
│   ├── relations.rs    # Relationships
│   └── migration.rs    # Migrations
├── utils/              # Utils layer (zero-dependency implementations)
│   ├── simple_pool.rs  # Simple connection pool
│   ├── simple_lru.rs   # LRU cache
│   ├── simple_error.rs # Simplified errors
│   └── simple_uuid.rs  # UUID/ID generation
└── monitoring/         # Monitoring layer
    ├── logger.rs       # Logging system
    └── performance.rs  # Performance monitoring
```

## 🚀 Quick Start

### Basic Usage

```rust
use torm::{Database, SqlValue};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Create a SQLite database (standard SQLite file format)
    let db = Database::sqlite("mydb.db").await?;

    // 2. Create a table
    db.execute("CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT, age INTEGER)", &[]).await?;

    // 3. Insert data (parameter binding supported)
    db.execute(
        "INSERT INTO users (name, age) VALUES (?, ?)",
        &[SqlValue::String("Alice".to_string()), SqlValue::I32(25)],
    ).await?;

    // 4. Query data
    let result = db.query("SELECT * FROM users WHERE age > ?", &[SqlValue::I32(20)]).await?;
    for row in &result.rows {
        println!("{:?}", row.get("name"));
    }

    // 5. Transactions
    let mut tx = db.begin_transaction().await?;
    tx.execute("INSERT INTO users (name, age) VALUES (?, ?)", &[
        SqlValue::String("Bob".to_string()),
        SqlValue::I32(30),
    ]).await?;
    tx.commit().await?;

    db.close().await?;
    Ok(())
}
```

The generated `mydb.db` is a standard SQLite file, directly inspectable with `sqlite3 mydb.db`:

```bash
$ sqlite3 mydb.db ".tables"
users
$ sqlite3 mydb.db "SELECT * FROM users;"
1|Alice|25
2|Bob|30
```

### Type-Safe SQL Values

```rust
let value: SqlValue = 42.into();                    // I32(42)
let value: SqlValue = "hello".into();               // String("hello")
let value: SqlValue = true.into();                  // Bool(true)
let value = SqlValue::DateTime(chrono::Utc::now()); // DateTime(...)

// SQL string conversion
let sql = value.to_sql_string();  // "42", "'hello'", "TRUE"
```

### Query Builder

```rust
use torm::QueryBuilder;

// Basic query
let (sql, bindings) = QueryBuilder::new("users")
    .where_eq("email", "john@example.com")
    .where_gt("age", 18)
    .order_by("created_at", "DESC")
    .limit(10)
    .build();
// sql: "SELECT * FROM users WHERE email = ? AND age > ? ORDER BY created_at DESC LIMIT 10"
```

### Connection Pool

```rust
use torm::Pool;

let config = torm::ConnectionConfig::sqlite("mydb.db")
    .with_max_connections(10);
let pool = Pool::sqlite("mydb.db", torm::PoolConfig::default()).await?;
let conn = pool.get_connection().await?;
```

## 📊 Database Support Status

### ✅ SQLite (Production-ready, standard file format)
- Built on rusqlite, generates standard SQLite files (sqlite3 compatible)
- Full CRUD operations
- Parameterized queries
- Transaction support
- Foreign key constraints
- **Status**: Ready for production

### ✅ Pure Rust In-Memory Engine (StorageEngine)
- Zero-dependency in-memory database
- Custom binary persistence format (TORMDB01)
- Full CRUD + WHERE conditions (AND/OR/comparison/LIKE)
- **Status**: Usable as a lightweight in-memory database

### ⚠️ MySQL (Framework support)
- TCP connection establishment
- MySQL protocol message structure
- Authentication protocol framework
- **Status**: Needs completion, suitable for learning

### ⚠️ PostgreSQL (Framework support)
- TCP connection establishment
- PostgreSQL StartupMessage
- Message send/receive framework
- **Status**: Needs completion, suitable for learning

## 🏃 Run Examples

```bash
# Basic usage example
cargo run --example basic_usage

# Complete feature demo
cargo run --example complete_demo

# Advanced features demo (relations, migrations, performance)
cargo run --example advanced_features

# Database integration example
cargo run --example integration_example

# Run tests
cargo test
```

## 🛠 Tech Stack

### External Dependencies
- **Async Runtime**: Tokio 1.53+
- **SQLite Implementation**: rusqlite 0.30 (bundled)
- **UUID Generation**: uuid 1.0
- **Serialization**: Serde 1.0
- **Time Handling**: Chrono 0.4

### Custom Implementations
- **Pure Rust Storage Engine**: StorageEngine (zero-dependency in-memory database)
- **MySQL Protocol**: MySqlConnection (framework)
- **PostgreSQL Protocol**: PostgresConnection (framework)
- **Type System**: SqlValue, Row, QueryResult
- **Connection Abstraction**: DatabaseConnection trait
- **Transaction System**: Transaction
- **Connection Pools**: Pool / SimplePool
- **Utilities**: SimpleUuid, SimpleLruCache, SimpleError

## 📚 Documentation

- [README.md](README.md) - English README
- [README.zh.md](README.zh.md) - Chinese README
- [DATABASE_REPLACEMENT.md](DATABASE_REPLACEMENT.md) - Database layer replacement details
- [DEPENDENCY_OPTIMIZATION.md](DEPENDENCY_OPTIMIZATION.md) - Dependency optimization details
- [PROJECT_SUMMARY.md](PROJECT_SUMMARY.md) - Project summary

## 🎓 Learning Value

TORM demonstrates:
- How to implement database protocols in Rust
- Type-safe database abstraction design
- Async I/O and network programming
- MySQL and PostgreSQL protocol fundamentals
- Production-grade SQLite implementation
- Zero-dependency utility libraries (UUID, LRU cache, connection pool)

## 🎯 Use Cases

### Production
- ✅ SQLite applications (mobile, desktop, lightweight web)
- ✅ Projects requiring standard SQLite file format (interoperable with other SQLite tools)
- ✅ Projects with strict dependency control

### Learning & Development
- ✅ Database protocol learning
- ✅ Rust async programming
- ✅ ORM design patterns

## 📝 License

MIT

## 🤝 Contributing

Issues and Pull Requests are welcome!
