# TORM - Tokio ORM 库

## 当前实现状态

### ✅ 已完成功能 (MVP - 第一阶段)

#### 1. 核心架构设计
- ✅ 定义 `DBDriver` 枚举（MySQL, PostgreSQL, SQLite）
- ✅ 实现 `Database` 结构体，包含连接池和驱动类型
- ✅ 设计 `Model` trait，所有模型需要实现此 trait
- ✅ 实现连接池管理（使用 `deadpool` 或 `sqlx` 的连接池）

#### 2. 连接与配置
- ✅ 实现 `Connect` 函数，支持不同数据库的 DSN
- ✅ 配置连接池参数（最大连接数、超时时间等）
- ✅ 实现数据库连接测试（Ping）
- ✅ 支持连接池的自动重连机制

#### 3. 查询操作
- ✅ `First` - 查询第一条记录（基础框架）
- ✅ `Find` - 查询多条记录（基础框架）
- ✅ `Count` - 统计记录数
- ✅ 支持链式调用
- ✅ 支持各种条件操作符

#### 4. 条件查询
- ✅ `Where` - 基本条件查询
- ✅ 支持链式调用
- ✅ 支持 `eq`, `ne`, `gt`, `gte`, `lt`, `lte`, `like` 等操作符
- ✅ 支持 `in`, `not_in`, `between` 等操作符
- ✅ 支持 `is_null`, `is_not_null`
- ✅ 支持原生 SQL 条件

#### 5. 排序与分页
- ✅ `Order` - 排序（ASC/DESC）
- ✅ `Limit` - 限制返回数量
- ✅ `Offset` - 偏移量
- ✅ 实现 `Paginate` - 分页查询

#### 6. 更新和删除操作
- ✅ `Update` - 更新操作（SQL 生成）
- ✅ `Delete` - 删除操作（SQL 生成）

#### 7. 事务支持
- ✅ `Begin` - 开始事务
- ✅ `Commit` - 提交事务
- ✅ `Rollback` - 回滚事务
- ✅ `Transaction` - 自动事务（闭包方式）

#### 8. Hooks（钩子）
- ✅ `BeforeCreate` / `AfterCreate`
- ✅ `BeforeUpdate` / `AfterUpdate`
- ✅ `BeforeDelete` / `AfterDelete`
- ✅ `BeforeFind` / `AfterFind`
- ✅ `BeforeSave` / `AfterSave`

#### 9. 时间戳自动管理
- ✅ `CreatedAt` - 创建时间
- ✅ `UpdatedAt` - 更新时间
- ✅ `DeletedAt` - 软删除时间

#### 10. 错误处理
- ✅ 定义统一的错误类型
- ✅ 错误码映射（数据库特定错误）

### 🔄 进行中/部分实现

#### CRUD 操作的完整实现
- 🔄 `Create` - SQL 生成部分完成，需要完善结果扫描
- 🔄 `Read` - 需要完善结果扫描和模型映射
- 🔄 `Update` - SQL 生成完成，需要执行部分
- 🔄 `Delete` - SQL 生成完成，需要执行部分

### ❌ 待实现功能

#### 关联关系
- ❌ `BelongsTo` - 属于
- ❌ `HasOne` - 有一个
- ❌ `HasMany` - 有多个
- ❌ `ManyToMany` - 多对多
- ❌ 关联的预加载（`Preload`）
- ❌ 关联的创建和更新
- ❌ 关联的删除

#### 高级查询功能
- ❌ `Or` - OR 条件（部分实现）
- ❌ `Not` - NOT 条件
- ❌ `Joins` - 表连接查询
- ❌ `Group` 和 `Having` - 分组查询
- ❌ `Distinct` - 去重查询
- ❌ `Pluck` - 查询单列
- ❌ `Scopes` - 复用查询条件

#### 数据迁移
- ✅ `AutoMigrate` - 基于 `Model::schema()` 自动建表与索引（`Database::auto_migrate`）
- ✅ `CreateTable` - 从 `TableDefinition` 生成 `CREATE TABLE IF NOT EXISTS`
- ✅ 索引支持 - `primaryKey` / `index` / `uniqueIndex` 字段标签自动创建索引（`CREATE [UNIQUE] INDEX IF NOT EXISTS`）
- ❌ `DropTable` - 删除表
- ❌ `AddColumn` - 添加列
- ❌ `DropColumn` - 删除列
- ❌ `ModifyColumn` - 修改列
- ❌ `RenameColumn` - 重命名列

#### 日志与性能
- ❌ SQL 日志记录（慢查询、错误查询）
- ❌ 执行时间统计
- ❌ 日志级别控制（Debug, Info, Warn, Error）
- ❌ 自定义 Logger 接口
- ❌ 查询缓存
- ❌ Prepared Statement 支持
- ❌ 批量操作优化

#### 数据类型支持
- ❌ JSON 类型支持
- ❌ 数组类型支持（PostgreSQL）
- ❌ UUID 支持
- ❌ 枚举类型支持
- ❌ 自定义类型（实现 Scanner 和 Valuer）

#### 工具与辅助
- ❌ `Raw` - 原生 SQL 执行
- ❌ `Exec` - 执行任意 SQL
- ❌ `Scan` - 扫描到自定义结构
- ❌ `Rows` - 获取原始行
- ❌ `WithContext` - 上下文支持
- ❌ `DryRun` - 预览生成的 SQL

#### 测试与文档
- ❌ 完整的单元测试
- ❌ 集成测试（MySQL, PostgreSQL, SQLite）
- ❌ 性能基准测试
- ❌ Mock 测试支持
- ❌ 详细的 API 文档
- ❌ 更多示例代码
- ❌ 最佳实践说明

## 核心文件结构

```
src/
├── lib.rs           # 库入口，导出公共API
├── main.rs          # 主程序入口
├── driver.rs        # 数据库驱动枚举和DSN配置
├── database.rs      # Database结构体和事务管理
├── model.rs         # Model trait和生命周期钩子
├── query.rs         # 查询构建器和Query API
├── pool.rs          # 连接池管理
└── error.rs         # 错误类型定义

examples/
└── basic_usage.rs   # 基本使用示例

tests/
└── integration_test.rs  # 集成测试
```

## 技术栈

- **异步运行时**: Tokio 1.53+
- **数据库驱动**: SQLx 0.8
- **连接池**: deadpool 0.12
- **序列化**: Serde 1.0
- **时间处理**: Chrono 0.4
- **UUID**: uuid 1.0
- **异步Trait**: async-trait 0.1
- **错误处理**: thiserror 1.0

## 使用示例

### 基本查询

```rust
use torm::Query;

let (sql, bindings) = Query::new("users")
    .where_eq("status", "active")
    .where_gt("age", "18")
    .order_by_desc("created_at")
    .limit(20)
    .build();
```

### 分页查询

```rust
let (sql, bindings) = Query::new("users")
    .paginate(2, 10)
    .build();
```

### 计数查询

```rust
let (sql, bindings) = Query::new("users")
    .where_eq("status", "active")
    .count();
```

### 更新操作

```rust
let mut updates = HashMap::new();
updates.insert("name".to_string(), "John Doe".to_string());

let (sql, bindings) = Query::new("users")
    .where_eq("id", "123")
    .update(&updates);
```

### 数据库连接

```rust
use torm::{Database, DBDriver, Dsn};

let dsn = Dsn::new(DBDriver::SQLite, "mydb.db");
let database = Database::new(&dsn.build(), DBDriver::SQLite).await?;
```

## 开发计划

### 第二阶段功能
- 关联关系实现
- 预加载机制
- 完整的数据迁移支持
- 更完善的 CRUD 操作

### 第三阶段功能
- 高级查询功能
- 批量操作优化
- 日志系统
- 自定义类型支持

### 第四阶段功能
- 性能优化
- 完整的测试套件
- 详细的文档和示例

## 注意事项

当前实现主要关注查询构建器和基础架构的搭建，完整的数据库操作功能还需要进一步完善，特别是：

1. 结果集的自动映射到模型
2. 关联关系的处理
3. 复杂查询的执行
4. 数据迁移的完整实现

建议在使用前先查看示例代码，了解当前功能的限制。