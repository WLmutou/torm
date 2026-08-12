// TORM Integration Examples
// 演示基于 `#[derive(Model)]` 结构体的高层数据库操作：
// create / first / all / update / delete / 条件查询（自动映射回类型）。
// 全程零 SqlValue。

use torm::db::database::Database;
use torm::{Model, Query};

/// 用户模型：所有字段自动映射，id 自动自增回填。
#[derive(Debug, Clone, Model)]
#[model(table_name = "users")]
pub struct User {
    pub id: i64,
    pub name: String,
    pub email: String,
    pub age: i32,
    pub active: bool,
}

impl User {
    fn new(name: &str, email: &str, age: i32, active: bool) -> Self {
        Self {
            id: 0,
            name: name.to_string(),
            email: email.to_string(),
            age,
            active,
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("TORM Integration Examples\n");

    // Example 1: SQLite 连接 + 依据模型自动建表
    println!("=== Example 1: Connection & AutoMigrate ===");
    let db = Database::sqlite(":memory:").await?;
    println!("Connected to SQLite: {}", db.config().database);
    db.auto_migrate::<User>().await?;
    println!("Created users table from model schema");

    // Example 2: 插入（create 自动回填自增 id）
    println!("\n=== Example 2: Create (Insert) ===");
    let mut alice = User::new("Alice", "alice@example.com", 25, true);
    db.create(&mut alice).await?;
    let mut bob = User::new("Bob", "bob@example.com", 30, true);
    db.create(&mut bob).await?;
    let mut carol = User::new("Carol", "carol@example.com", 35, true);
    db.create(&mut carol).await?;
    let mut dave = User::new("Dave", "dave@example.com", 17, false);
    db.create(&mut dave).await?;
    println!("  Inserted: Alice={} Bob={} Carol={} Dave={}",
        alice.id, bob.id, carol.id, dave.id);

    // Example 3: 条件查询 + 自动映射回 Vec<User>
    println!("\n=== Example 3: Typed Query (auto-mapped) ===");
    let adults: Vec<User> = Query::new("users")
        .where_gt("age", 18)
        .order_by_desc("age")
        .query(&db)
        .models::<User>()
        .await?;
    println!("  Adults (age > 18): {}", adults.len());
    for u in &adults {
        println!("    - {} <{}> age={} active={}", u.name, u.email, u.age, u.active);
    }

    // Example 4: 按主键读取 / 读取全部
    println!("\n=== Example 4: First & All ===");
    let one: Option<User> = db.first::<User>(&alice.id.to_string()).await?;
    println!("  First(id={}): {:?}", alice.id, one.map(|u| u.name));
    let all: Vec<User> = db.all::<User>().await?;
    println!("  All: {} users", all.len());

    // Example 5: 更新（直接执行，返回影响行数）
    println!("\n=== Example 5: Update ===");
    let affected = db.update(&mut alice, &[("age", 26)]).await?;
    println!("  Updated Alice age -> affected {} row(s)", affected);
    let updated: User = db.first(&alice.id.to_string()).await?.expect("exists");
    println!("  Alice age now = {}", updated.age);

    // Example 6: 删除
    println!("\n=== Example 6: Delete ===");
    let removed = db.delete(&mut dave).await?;
    println!("  Deleted Dave -> {} row(s)", removed);
    let remaining = db.all::<User>().await?;
    println!("  Remaining: {} users", remaining.len());

    // Example 7: 计数查询
    println!("\n=== Example 7: Count ===");
    let count = Query::new("users").query(&db).count().await?;
    let n = count
        .rows
        .first()
        .and_then(|r| r.get("COUNT(*)").or_else(|| r.get("count")))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    println!("  Total users = {}", n);

    db.close().await?;
    println!("\n=== Examples Complete ===");
    Ok(())
}
