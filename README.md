# TORM - Tokio ORM

TORM 是一个基于 Tokio 异步运行时的 Rust ORM（对象关系映射）库，提供类似 GORM 的功能，用纯 Rust 实现数据库层。

## 🎯 核心特性

- ✅ **自定义数据库实现** - 用 Rust 替代 sqlx 库
- ✅ **SQLite 完整支持** - 生产级 SQLite 实现
- ✅ **MySQL/PostgreSQL 框架** - 协议框架支持
- ✅ **最小化依赖** - 仅 6 个核心依赖
- ✅ **异步/await 支持** - 完全基于 Tokio 异步运行时
- ✅ **多数据库支持** - MySQL、PostgreSQL、SQLite
- ✅ **流畅的查询构建器** - 提供简洁直观的查询 API
- ✅ **模型 Trait** - 自动管理创建时间、更新时间等时间戳
- ✅ **事务支持** - 支持事务的创建、提交和回滚
- ✅ **复杂查询** - 支持分页、排序、多条件查询等

## 📦 依赖 (极简优化)

```toml
[dependencies]
tokio = "1.53"              # 异步运行时
rusqlite = "0.30"           # SQLite 实现
uuid = "1.0"                # UUID 生成
serde = "1.0"               # 序列化
serde_json = "1.0"          # JSON 支持
chrono = "0.4"              # 时间处理
```

### 🎯 数据库层实现

| 功能 | 实现方式 | 状态 |
|------|----------|------|
| SQLite | rusqlite + 自定义抽象 | ✅ 完整 |
| MySQL | 自定义协议框架 | ⚠️ 框架 |
| PostgreSQL | 自定义协议框架 | ⚠️ 框架 |
| 类型安全 | 自定义 SqlValue | ✅ 完整 |
| 事务支持 | 自定义实现 | ✅ 完整 |

## 🚀 快速开始

### 基本使用

```rust
use torm::{ConnectionConfig, ConnectionFactory, SqlValue};
use torm::SimpleUuid;

#[tokio::main]
async fn main() -> torm::SimpleResult<()> {
    // 1. 创建数据库连接
    let config = ConnectionConfig::sqlite("mydb.db");
    let conn = ConnectionFactory::create_connection(config).await?;
    
    // 2. 生成 UUID
    let user_id = SimpleUuid::new_v4();
    println!("Generated ID: {}", user_id);
    
    // 3. 执行查询
    let result = conn.execute_query("SELECT * FROM users WHERE status = ?", 
                                   &[SqlValue::String("active".to_string())]).await?;
    
    for row in result.rows {
        println!("User: {:?}", row.get("name"));
    }
    
    Ok(())
}
```

### SQLite 生产级支持

```rust
// SQLite - 完整支持
let config = ConnectionConfig::sqlite("mydb.db");
let conn = ConnectionFactory::create_connection(config).await?;

// 创建表
conn.execute(
    "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, created_at TEXT)",
    &[]
).await?;

// 插入数据
conn.execute(
    "INSERT INTO users (name, created_at) VALUES (?, ?)",
    &[
        SqlValue::String("John".to_string()),
        SqlValue::DateTime(chrono::Utc::now())
    ]
).await?;

// 查询数据
let result = conn.execute_query("SELECT * FROM users", &[]).await?;
for row in result.rows {
    let name: Option<&str> = row.get("name").and_then(|v| v.as_str());
    println!("User: {:?}", name);
}

// 事务支持
let mut tx = conn.begin_transaction().await?;
tx.execute("INSERT INTO logs (message) VALUES (?)", 
          &[SqlValue::String("Transaction test".to_string())]).await?;
tx.commit().await?;
```

## 📚 数据库层特性

### 类型安全的 SQL 值

```rust
// 自动类型转换
let value: SqlValue = 42.into();                    // I32(42)
let value: SqlValue = "hello".into();               // String("hello")
let value: SqlValue = true.into();                   // Bool(true)
let value: SqlValue = chrono::Utc::now().into();     // DateTime(...)

// SQL 字符串转换
let sql = value.to_sql_string();  // "42", "'hello'", "TRUE"
```

### 统一错误处理

```rust
pub enum DbError {
    ConnectionError(String),
    QueryError(String),
    ExecutionError(String),
    TransactionError(String),
    // ...
}

// 简化的错误创建
let error = DbError::connection_error("Failed to connect");
let error = DbError::query_error("Invalid SQL");
```

### 事务支持

```rust
// 显式事务
let mut tx = conn.begin_transaction().await?;
tx.execute("INSERT INTO logs (message) VALUES (?)", 
          &[SqlValue::String("test".to_string())]).await?;
tx.commit().await?;

// 自动回滚
{
    let tx = conn.begin_transaction().await?;
    // 操作...
} // 自动回滚
```

## 📊 数据库支持状态

### ✅ SQLite (生产级)
- 完整的 CRUD 操作
- 参数化查询
- 事务支持
- 外键约束
- 类型转换
- **状态**: 可用于生产环境

### ⚠️ MySQL (框架支持)
- TCP 连接建立
- MySQL 协议消息结构
- 认证协议框架
- **状态**: 需要完善，可用作学习

### ⚠️ PostgreSQL (框架支持)
- TCP 连接建立
- PostgreSQL StartupMessage
- 消息发送/接收框架
- **状态**: 需要完善，可用作学习

## 🏃 运行示例

```bash
# 运行基本使用示例
cargo run --example basic_usage

# 运行测试
cargo test
```

## 🛠 技术栈

### 外部依赖 (仅 6 个)
- **异步运行时**: Tokio 1.53+
- **SQLite 实现**: rusqlite 0.30
- **UUID 生成**: uuid 1.0
- **序列化**: Serde 1.0
- **时间处理**: Chrono 0.4

### 自定义实现
- **MySQL 协议**: MySqlConnection (框架)
- **PostgreSQL 协议**: PostgresConnection (框架)
- **SQLite 完整实现**: SqliteConnection (生产级)
- **数据类型系统**: SqlValue, Row, QueryResult
- **连接抽象**: DatabaseConnection trait
- **事务系统**: Transaction

## 📚 文档

- [README.md](README.md) - 项目概述
- [DATABASE_REPLACEMENT.md](DATABASE_REPLACEMENT.md) - 数据库层替换详情
- [DEPENDENCY_OPTIMIZATION.md](DEPENDENCY_OPTIMIZATION.md) - 依赖优化详情

## 🎓 学习价值

TORM 展示了：
- 如何用 Rust 实现数据库协议
- 类型安全的数据库抽象设计
- 异步 I/O 和网络编程
- MySQL 和 PostgreSQL 协议基础
- 生产级的 SQLite 实现

## 🎯 适用场景

### 生产环境
- ✅ SQLite 应用（移动、桌面、轻量级 Web）
- ✅ 需要自定义数据库操作的应用
- ✅ 对依赖有严格控制的项目

### 学习开发
- ✅ 数据库协议学习
- ✅ Rust 异步编程学习
- ✅ ORM 设计模式学习

## 📝 许可证

MIT

## 🤝 贡献

欢迎提交 Issue 和 Pull Request！