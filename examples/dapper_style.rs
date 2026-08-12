//! 演示 Dapper 风格的类型化 CRUD —— insert / query / update 全程无需手写 `SqlValue`。
//!
//! 运行方式：`cargo run --example dapper_style`
//!
//! 核心体验：
//! - `Database::create(&mut user)` → 插入并回填自增主键（零 SqlValue）
//! - `Query::new(...).query(&db).models::<User>()` → 条件查询自动映射回 `Vec<User>`
//! - `Database::update(&mut user, &[("age", 30)])` → 直接执行并返回影响行数（零 SqlValue）
//! - `Database::all::<User>()` / `first::<User>(id)` → 全表/按主键查询

use torm::{Database, Model, Query};

/// 用户模型：所有字段自动映射，无需手写 `columns()` / `from_row()`。
#[derive(Debug, Clone, Model)]
#[model(table_name = "users")]
pub struct User {
    pub id: i64,
    pub name: String,
    pub email: String,
    pub age: i32,
}

impl User {
    fn new(name: &str, email: &str, age: i32) -> Self {
        Self {
            id: 0, // 0 触发自增主键，insert 后自动回填
            name: name.to_string(),
            email: email.to_string(),
            age,
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::sqlite(":memory:").await?;

    // 自动建表（依据模型 schema）
    db.auto_migrate::<User>().await?;
    println!("表 users 已自动创建\n");

    // === 1. insert：无需手写 SqlValue，主键自动自增并回填 ===
    println!("=== 1. Insert (自增主键自动回填) ===");
    let mut alice = User::new("Alice", "alice@example.com", 25);
    db.create(&mut alice).await?;
    let mut bob = User::new("Bob", "bob@example.com", 17);
    db.create(&mut bob).await?;
    let mut carol = User::new("Carol", "carol@example.com", 30);
    db.create(&mut carol).await?;
    println!(
        "  id 自动分配: Alice={} Bob={} Carol={}\n",
        alice.id, bob.id, carol.id
    );

    // === 2. query：条件查询自动映射回 Vec<User> ===
    println!("=== 2. Query (自动映射回 User) ===");
    let adults: Vec<User> = Query::new("users")
        .where_gte("age", 18)
        .order_by_desc("age")
        .query(&db)
        .models::<User>()
        .await?;
    for u in &adults {
        println!("  id={} name={} email={} age={}", u.id, u.name, u.email, u.age);
    }

    // 分页 + 条件
    let first_two: Vec<User> = Query::new("users")
        .where_gt("age", 0)
        .order_by_asc("id")
        .limit(2)
        .query(&db)
        .models::<User>()
        .await?;
    println!("  前 2 条: {:?}", first_two.iter().map(|u| &u.name).collect::<Vec<_>>());
    println!();

    // === 3. update：直接执行 SQL，返回影响行数 ===
    println!("=== 3. Update ===");
    let affected = db.update(&mut alice, &[("age", 26)]).await?;
    println!("  更新 age 影响 {} 行", affected);

    // 多列同类型（全为 &str）可直接批量更新
    db.update(
        &mut alice,
        &[("email", "alice_new@example.com"), ("name", "Alice A.")],
    )
    .await?;

    // 重新查询验证
    let refreshed: User = db.first::<User>(&alice.id.to_string()).await?.unwrap();
    println!(
        "  更新后: name={} age={} email={}\n",
        refreshed.name, refreshed.age, refreshed.email
    );

    // === 4. 条件删除 ===
    println!("=== 4. Delete ===");
    let removed = db.delete(&mut bob).await?;
    println!("  删除 Bob 影响 {} 行", removed);
    let remaining = db.all::<User>().await?;
    println!("  剩余 {} 条: {:?}", remaining.len(), remaining.iter().map(|u| &u.name).collect::<Vec<_>>());

    println!("\n✅ insert / query / update / delete 全程零 SqlValue，Dapper 风格完成！");
    Ok(())
}
