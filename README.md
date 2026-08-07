# TORM - Tokio ORM

TORM is a Rust ORM (Object-Relational Mapping) library built on the Tokio async runtime, providing GORM-like functionality with a layered module design (Database / ORM / Utils / Monitoring).

## 🎯 Key Features

- ✅ **Standard SQLite Support** - Built on rusqlite, generates standard SQLite file format (readable by sqlite3 and other SQLite tools)
- ✅ **Pure Rust Storage Engine** - Built-in zero-dependency in-memory storage engine (StorageEngine)
- ✅ **PostgreSQL Support** - Native wire protocol implementation (cleartext / MD5 / SCRAM-SHA-256 auth, parameterized queries)
- ✅ **MySQL Support** - Native wire protocol implementation (mysql_native_password / caching_sha2_password / sha256_password auth, text/binary protocol parameterized queries)
- ✅ **Async/await Support** - Fully based on the Tokio async runtime
- ✅ **Multi-Database Support** - MySQL, PostgreSQL, SQLite
- ✅ **Fluent Query Builder** - Clean and intuitive query API
- ✅ **Query Direct Execution** - `insert` / `update` / `delete` execute SQL directly, inspect with `return_sql()`
- ✅ **Advanced Queries** - JOIN, GROUP BY, HAVING, aggregate functions
- ✅ **Model Trait** - Automatic management of created_at, updated_at timestamps
- ✅ **`#[derive(Model)]` Macro** - Generate the `Model` impl from a plain struct, eliminating boilerplate
- ✅ **GORM-style Model CRUD** - `create_model` / `first_model` / `find_models` / `update_model` / `delete_model` on `Database`
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
# PostgreSQL / MySQL wire protocol authentication
sha2 = "0.10"               # PostgreSQL SCRAM-SHA-256 / MySQL caching_sha2_password
sha1 = "0.10"               # MySQL mysql_native_password authentication
md-5 = "0.10"               # PostgreSQL MD5 authentication
hex = "0.4"                 # Byte/hex encoding
base64 = "0.22"             # SCRAM base64 encoding
# RSA encryption for MySQL caching_sha2_password full auth (MySQL 8.0+)
rsa = "0.9"
num-bigint = "0.4"
rand = "0.8"
```

### Database Layer Implementation

| Feature | Implementation | Status |
|---------|---------------|--------|
| SQLite | rusqlite (standard file format) | ✅ Complete |
| In-memory engine | Pure Rust StorageEngine | ✅ Complete |
| MySQL | Custom wire protocol (native) | ✅ Complete |
| PostgreSQL | Custom wire protocol (native) | ✅ Complete |
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
│   ├── mysql.rs        # MySQL wire protocol implementation
│   ├── postgresql.rs   # PostgreSQL wire protocol implementation
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

`Query` provides a fluent builder that can **execute directly** against a `&Database`, or **inspect** the generated SQL with `return_sql()`.

```rust
use torm::{Database, Query, SqlValue};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::sqlite("mydb.db").await?;
    db.execute(
        "CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT, age INTEGER)",
        &[],
    ).await?;

    // ---- Writes execute directly (INSERT / UPDATE / DELETE) ----
    let q = Query::new("users").where_eq("name", SqlValue::String("Alice".to_string()));

    let affected = q.update(
        &{ let mut m = std::collections::HashMap::new();
           m.insert("age".to_string(), SqlValue::I32(31)); m },
        &db,
    ).await?;                                // executes UPDATE, returns affected rows

    // Inspect the SQL & params of the last operation
    let (sql, params) = q.return_sql();
    // sql: "UPDATE users SET age = ? WHERE name = ?"

    // Insert / delete execute the same way
    Query::new("users").insert(
        &[("name", SqlValue::String("Bob".to_string())),
          ("age", SqlValue::I32(25))],
        &db,
    ).await?;
    Query::new("users").where_eq("age", SqlValue::I32(25)).delete(&db).await?;

    // ---- Reads: QueryExecutor via query(db), or SqlStatement via build() ----
    let result = Query::new("users").query(&db).select().await?;  // executes SELECT
    let total = Query::new("users").query(&db).count().await?     // executes SELECT COUNT(*)
        .rows.first().and_then(|r| r.get("COUNT(*)")).and_then(|v| v.as_i64()).unwrap_or(0);

    // build().query() also works, and return_sql() inspects the SQL
    let result = Query::new("users").where_gt("age", SqlValue::I32(20)).build()
        .query(&db).await?;                  // executes SELECT
    let (sql, _) = Query::new("users").count().return_sql();
    // sql: "SELECT COUNT(*) FROM users"

    Ok(())
}
```

`Query::query(db)` returns a **`QueryExecutor`** for chaining read operations:

- `QueryExecutor::count()` - executes `SELECT COUNT(*)`, returning a result set with a `COUNT(*)` column
- `QueryExecutor::select()` - executes `SELECT *`

`Query` also returns a `SqlStatement` from `build()` / `count()` / `build_update()` / etc., which offers both execution and inspection:

- `SqlStatement::execute(&db)` / `SqlStatement::query(&db)` - run the statement directly
- `SqlStatement::return_sql()` - get the `(sql, params)` pair
- `Query::return_sql()` - get the `(sql, params)` of the most recently built / executed operation

> **Note**: SQLite and MySQL use `?` placeholders; PostgreSQL uses `$1/$2/...`. The conversion happens automatically during execution.

### Deriving a Model

Instead of hand-writing the `Model` impl, annotate your struct with `#[derive(Model)]` and a `#[model(table_name = "...")]` attribute. The macro generates `columns()`, `from_row()`, primary-key accessors, and timestamp accessors for you.

```rust
use torm::{Model, Timestamps};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Model)]
#[model(table_name = "users")]
pub struct User {
    pub id: i64,                                        // primary key -> id() / set_id()
    pub name: String,
    pub age: Option<i32>,
    #[model(column = "created_at")]
    pub created_at: Option<DateTime<Utc>>,              // standalone timestamp field
    pub timestamps: Timestamps,                          // or a Timestamps struct
    #[model(skip)]
    pub role_ids: Option<Vec<i64>>,                      // non-DB field, auto-skipped
}
```

Supported field types: `String`, `bool`, `i8/i16/i32/i64`, `f32/f64`, `chrono::DateTime<Utc>`, `Uuid`, `Vec<u8>` and their `Option<...>` wrappers. Other types are skipped automatically; use `#[model(skip)]` to exclude a field explicitly, and `#[model(column = "...")]` to rename a DB column.

### Connection Pool

```rust
use torm::Pool;

let config = torm::ConnectionConfig::sqlite("mydb.db")
    .with_max_connections(10);
let pool = Pool::sqlite("mydb.db", torm::PoolConfig::default()).await?;
let conn = pool.get_connection().await?;
```

### MySQL Connection

```rust
use torm::{Database, SqlValue};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to MySQL (native protocol, supports mysql_native_password / caching_sha2_password)
    let db = Database::mysql("localhost", 3306, "mydb", "odoo", "odoo").await?;

    // Parameterized query (COM_STMT_PREPARE / COM_STMT_EXECUTE binary protocol)
    db.execute(
        "INSERT INTO users (name, age) VALUES (?, ?)",
        &[SqlValue::String("Alice".to_string()), SqlValue::I32(30)],
    ).await?;

    let result = db.query("SELECT * FROM users WHERE age > ?", &[SqlValue::I32(18)]).await?;
    for row in &result.rows {
        println!("{:?}", row.get("name"));
    }

    db.close().await?;
    Ok(())
}
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

### ✅ MySQL (Native wire protocol, production-ready)
- Real TCP connection via `tokio::net::TcpStream`
- Full initial handshake (Protocol 10) and handshake response
- Authentication: `mysql_native_password`, `caching_sha2_password` (fast/full auth with RSA encryption), `sha256_password`
- AuthSwitchRequest / AuthMoreData auth exchange flow
- Text protocol (`COM_QUERY`) for parameterless queries
- Binary protocol (`COM_STMT_PREPARE` / `COM_STMT_EXECUTE`) for parameterized queries
- Column definition, text-row / binary-row decoding, OK/EOF/Error packets
- Supports `CLIENT_DEPRECATE_EOF` (MySQL 5.7+) and classic EOF protocol
- Transactions (BEGIN / COMMIT / ROLLBACK)
- **Status**: Ready for production use with MySQL 5.7+

### ✅ PostgreSQL (Native wire protocol, production-ready)
- Real TCP connection via `tokio::net::TcpStream`
- Full startup handshake (StartupMessage, protocol 3.0)
- Authentication: cleartext, MD5, SCRAM-SHA-256 (with server signature verification)
- Simple query protocol (`Q`) for multi-statement SQL
- Extended query protocol (Parse/Bind/Describe/Execute/Sync) for parameterized statements
- Row decoding: bool, int2/4/8, float4/8, text/varchar, bytea, json/jsonb, date/timestamp/timestamptz, numeric
- Transactions (BEGIN / COMMIT / ROLLBACK)
- **Status**: Ready for production use with PostgreSQL 10+

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
- **MySQL Protocol**: MySqlConnection (native wire protocol)
- **PostgreSQL Protocol**: PostgresConnection (native wire protocol)
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
- ✅ MySQL applications (web services, enterprise apps, supports MySQL 5.7+)
- ✅ PostgreSQL applications (web services, enterprise apps, supports PostgreSQL 10+)
- ✅ Projects with strict dependency control

### Learning & Development
- ✅ Database protocol learning
- ✅ Rust async programming
- ✅ ORM design patterns

## 📝 License

MIT

## 🤝 Contributing

Issues and Pull Requests are welcome!
