# TORM - Tokio ORM

TORM 是一个基于 Tokio 异步运行时的 Rust ORM（对象关系映射）库，提供类似 GORM 的功能，采用分层模块设计（数据库层 / ORM 层 / 工具层 / 监控层）。

## 🎯 核心特性

- ✅ **标准 SQLite 支持** - 基于 rusqlite，生成标准 SQLite 文件格式（可被 sqlite3 等工具直接读取）
- ✅ **纯 Rust 存储引擎** - 内置零依赖的内存存储引擎（StorageEngine）
- ✅ **PostgreSQL 支持** - 原生协议实现（cleartext / MD5 / SCRAM-SHA-256 认证、参数化查询）
- ✅ **MySQL 支持** - 原生协议实现（mysql_native_password / caching_sha2_password / sha256_password 认证、文本/二进制协议参数化查询）
- ✅ **异步/await 支持** - 完全基于 Tokio 异步运行时
- ✅ **多数据库支持** - MySQL、PostgreSQL、SQLite
- ✅ **流畅的查询构建器** - 提供简洁直观的查询 API
- ✅ **高级查询** - JOIN、GROUP BY、HAVING、聚合函数
- ✅ **模型 Trait** - 自动管理创建时间、更新时间等时间戳
- ✅ **GORM 风格模型 CRUD** - `Database` 上的 `create_model` / `first_model` / `find_models` / `update_model` / `delete_model`
- ✅ **事务支持** - 支持事务的创建、提交和回滚
- ✅ **连接池** - 支持 SQLite/MySQL/PostgreSQL 连接池
- ✅ **日志与性能监控** - 内置日志系统和性能统计

## 📦 依赖

```toml
[dependencies]
tokio = "1.53"              # 异步运行时
rusqlite = { version = "0.30", features = ["bundled"] }  # SQLite（标准文件格式）
uuid = "1.0"                # UUID 生成
serde = "1.0"               # 序列化
serde_json = "1.0"          # JSON 支持
chrono = "0.4"              # 时间处理
async-trait = "0.1"         # 异步 trait
thiserror = "1.0"           # 错误派生
# PostgreSQL / MySQL 协议认证
sha2 = "0.10"               # PostgreSQL SCRAM-SHA-256 / MySQL caching_sha2_password
sha1 = "0.10"               # MySQL mysql_native_password 认证
md-5 = "0.10"               # PostgreSQL MD5 认证
hex = "0.4"                 # 字节/十六进制编码
base64 = "0.22"             # SCRAM base64 编码
# MySQL caching_sha2_password 全量认证的 RSA 加密（MySQL 8.0+）
rsa = "0.9"
num-bigint = "0.4"
rand = "0.8"
```

### 数据库层实现

| 功能 | 实现方式 | 状态 |
|------|----------|------|
| SQLite | rusqlite（标准文件格式） | ✅ 完整 |
| 内存存储引擎 | 纯 Rust StorageEngine | ✅ 完整 |
| MySQL | 自定义协议（原生实现） | ✅ 完整 |
| PostgreSQL | 自定义协议（原生实现） | ✅ 完整 |
| 类型安全 | 自定义 SqlValue | ✅ 完整 |
| 事务支持 | 自定义实现 | ✅ 完整 |

## 🏗 模块结构

```
src/
├── lib.rs              # 模块声明与导出入口
├── db/                 # 数据库层
│   ├── db_types.rs     # SQL 类型系统 (SqlValue, Row, QueryResult)
│   ├── database.rs     # 连接抽象、事务、连接工厂、Database
│   ├── driver.rs       # DBDriver, Dsn
│   ├── error.rs        # TormError
│   ├── storage.rs      # 纯 Rust 内存存储引擎
│   ├── sqlite.rs       # SQLite 实现（rusqlite 后端）
│   ├── mysql.rs        # MySQL 协议实现
│   ├── postgresql.rs   # PostgreSQL 协议实现
│   └── pool.rs         # 连接池
├── orm/                # ORM 层
│   ├── model.rs        # Model trait
│   ├── query.rs        # Query/QueryBuilder
│   ├── advanced_query.rs # 高级查询 (JOIN/GROUP BY/聚合)
│   ├── relations.rs    # 关联关系
│   └── migration.rs    # 数据迁移
├── utils/              # 工具层（零依赖实现）
│   ├── simple_pool.rs  # 简单连接池
│   ├── simple_lru.rs   # LRU 缓存
│   ├── simple_error.rs # 简化错误
│   └── simple_uuid.rs  # UUID/ID 生成
└── monitoring/         # 监控层
    ├── logger.rs       # 日志系统
    └── performance.rs  # 性能监控
```

## 🚀 快速开始

### 基本使用

```rust
use torm::{Database, SqlValue};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 创建 SQLite 数据库（标准 SQLite 文件格式）
    let db = Database::sqlite("mydb.db").await?;

    // 2. 创建表
    db.execute("CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT, age INTEGER)", &[]).await?;

    // 3. 插入数据（支持参数绑定）
    db.execute(
        "INSERT INTO users (name, age) VALUES (?, ?)",
        &[SqlValue::String("Alice".to_string()), SqlValue::I32(25)],
    ).await?;

    // 4. 查询数据
    let result = db.query("SELECT * FROM users WHERE age > ?", &[SqlValue::I32(20)]).await?;
    for row in &result.rows {
        println!("{:?}", row.get("name"));
    }

    // 5. 事务支持
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

生成的 `mydb.db` 是标准 SQLite 文件，可用 `sqlite3 mydb.db` 直接查看：

```bash
$ sqlite3 mydb.db ".tables"
users
$ sqlite3 mydb.db "SELECT * FROM users;"
1|Alice|25
2|Bob|30
```

### 类型安全的 SQL 值

```rust
let value: SqlValue = 42.into();                    // I32(42)
let value: SqlValue = "hello".into();               // String("hello")
let value: SqlValue = true.into();                  // Bool(true)
let value = SqlValue::DateTime(chrono::Utc::now()); // DateTime(...)

// SQL 字符串转换
let sql = value.to_sql_string();  // "42", "'hello'", "TRUE"
```

### 查询构建器

```rust
use torm::QueryBuilder;

// 基本查询
let (sql, bindings) = QueryBuilder::new("users")
    .where_eq("email", "john@example.com")
    .where_gt("age", 18)
    .order_by("created_at", "DESC")
    .limit(10)
    .build();
// sql: "SELECT * FROM users WHERE email = ? AND age > ? ORDER BY created_at DESC LIMIT 10"
```

### 连接池

```rust
use torm::Pool;

let config = torm::ConnectionConfig::sqlite("mydb.db")
    .with_max_connections(10);
let pool = Pool::sqlite("mydb.db", torm::PoolConfig::default()).await?;
let conn = pool.get_connection().await?;
```

### MySQL 连接

```rust
use torm::{Database, SqlValue};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 连接 MySQL（原生协议，支持 mysql_native_password / caching_sha2_password）
    let db = Database::mysql("localhost", 3306, "mydb", "odoo", "odoo").await?;

    // 参数化查询（COM_STMT_PREPARE / COM_STMT_EXECUTE 二进制协议）
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

## 📊 数据库支持状态

### ✅ SQLite（生产级，标准文件格式）
- 基于 rusqlite，生成标准 SQLite 文件（sqlite3 兼容）
- 完整的 CRUD 操作
- 参数化查询
- 事务支持
- 外键约束
- **状态**: 可用于生产环境

### ✅ 纯 Rust 内存引擎（StorageEngine）
- 零依赖内存数据库
- 自定义二进制持久化格式（TORMDB01）
- 完整的 CRUD + WHERE 条件（AND/OR/比较运算/LIKE）
- **状态**: 可用作轻量级内存数据库

### ✅ MySQL（原生协议，生产可用）
- 通过 `tokio::net::TcpStream` 建立真实 TCP 连接
- 完整的初始握手（Protocol 10）与握手响应
- 认证：`mysql_native_password`、`caching_sha2_password`（快速/全量认证，含 RSA 加密）、`sha256_password`
- AuthSwitchRequest / AuthMoreData 认证切换流程
- 文本协议（`COM_QUERY`）执行无参数查询
- 二进制协议（`COM_STMT_PREPARE` / `COM_STMT_EXECUTE`）支持参数化查询
- 列定义、文本行/二进制行解码、OK/EOF/Error 包
- 支持 `CLIENT_DEPRECATE_EOF`（MySQL 5.7+）与经典 EOF 协议
- 事务（BEGIN / COMMIT / ROLLBACK）
- **状态**: 可用于 MySQL 5.7+ 生产环境

### ✅ PostgreSQL（原生协议，生产可用）
- 通过 `tokio::net::TcpStream` 建立真实 TCP 连接
- 完整启动握手（StartupMessage，协议 3.0）
- 认证：cleartext、MD5、SCRAM-SHA-256（含服务端签名校验）
- 简单查询协议（`Q`），支持多语句 SQL
- 扩展查询协议（Parse/Bind/Describe/Execute/Sync），支持参数化语句
- 行解码：bool、int2/4/8、float4/8、text/varchar、bytea、json/jsonb、date/timestamp/timestamptz、numeric
- 事务（BEGIN / COMMIT / ROLLBACK）
- **状态**: 可用于 PostgreSQL 10+ 生产环境

## 🏃 运行示例

```bash
# 基本使用示例
cargo run --example basic_usage

# 完整功能演示
cargo run --example complete_demo

# 高级功能演示（关联、迁移、性能）
cargo run --example advanced_features

# 数据库集成示例
cargo run --example integration_example

# 运行测试
cargo test
```

## 🛠 技术栈

### 外部依赖
- **异步运行时**: Tokio 1.53+
- **SQLite 实现**: rusqlite 0.30（bundled）
- **UUID 生成**: uuid 1.0
- **序列化**: Serde 1.0
- **时间处理**: Chrono 0.4

### 自定义实现
- **纯 Rust 存储引擎**: StorageEngine（零依赖内存数据库）
- **MySQL 协议**: MySqlConnection（原生协议实现）
- **PostgreSQL 协议**: PostgresConnection（原生协议实现）
- **数据类型系统**: SqlValue, Row, QueryResult
- **连接抽象**: DatabaseConnection trait
- **事务系统**: Transaction
- **连接池**: Pool / SimplePool
- **工具库**: SimpleUuid, SimpleLruCache, SimpleError

## 📚 文档

- [README.md](README.md) - English README
- [README.zh.md](README.zh.md) - 中文 README
- [DATABASE_REPLACEMENT.md](DATABASE_REPLACEMENT.md) - 数据库层替换详情
- [DEPENDENCY_OPTIMIZATION.md](DEPENDENCY_OPTIMIZATION.md) - 依赖优化详情
- [PROJECT_SUMMARY.md](PROJECT_SUMMARY.md) - 项目总结

## 🎓 学习价值

TORM 展示了：
- 如何用 Rust 实现数据库协议
- 类型安全的数据库抽象设计
- 异步 I/O 和网络编程
- MySQL 和 PostgreSQL 协议基础
- 生产级的 SQLite 实现
- 零依赖工具库的实现（UUID、LRU 缓存、连接池）

## 🎯 适用场景

### 生产环境
- ✅ SQLite 应用（移动、桌面、轻量级 Web）
- ✅ 需要标准 SQLite 文件格式的项目（可与其他 SQLite 工具互操作）
- ✅ MySQL 应用（Web 服务、企业应用，支持 MySQL 5.7+）
- ✅ PostgreSQL 应用（Web 服务、企业应用，支持 PostgreSQL 10+）
- ✅ 对依赖有严格控制的项目

### 学习开发
- ✅ 数据库协议学习
- ✅ Rust 异步编程学习
- ✅ ORM 设计模式学习

## 📝 许可证

MIT

## 🤝 贡献

欢迎提交 Issue 和 Pull Request！
