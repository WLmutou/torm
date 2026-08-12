
//! 运行方式：`cargo run --example ergonomic_query`
//!
//! 核心改进：所有条件值（`where_eq` / `where_gt` / `where_like` / `where_in`
//! 等）都通过 `impl Into<SqlValue>` 自动接收原生 Rust 值，无需手写
//! `SqlValue::String(...)` / `SqlValue::I32(...)`。
//!
//! ```text
//! 现在：Query::new("users").where_eq("age", 18).where_like("name", "A%")
//! ```

use torm::{AdvancedQuery, Database, Query, WhereCondition};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::sqlite(":memory:").await?;

    db.execute(
        "CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT,
            age INTEGER,
            score REAL,
            active INTEGER
        )",
        &[],
    )
    .await?;

    // 插入示例数据（raw SQL 参数用 .into() 由原生值自动转换）
    for (name, age, score, active) in [
        ("Alice", 25i64, 88.5f64, 1),
        ("Bob", 17, 76.2, 0),
        ("Carol", 30, 95.0, 1),
        ("Dave", 22, 63.8, 1),
        ("Eve", 41, 91.3, 0),
    ] {
        db.execute(
            "INSERT INTO users (name, age, score, active) VALUES (?, ?, ?, ?)",
            &[name.into(), age.into(), score.into(), active.into()],
        )
        .await?;
    }

    println!("=== 1. 基础链式条件（自动类型转换）===");
    let result = Query::new("users")
        .where_gte("age", 18)          // i32 自动转
        .where_gt("score", 80.0)       // f64 自动转
        .where_like("name", "A%")      // &str 自动转
        .query(&db)
        .select()
        .await?;
    for row in &result.rows {
        println!(
            "  name={} age={} score={}",
            row.get("name").and_then(|v| v.as_str()).unwrap_or(""),
            row.get("age").and_then(|v| v.as_i64()).unwrap_or(0),
            row.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0),
        );
    }

    println!("\n=== 2. 整型 / 浮点 / 字符串全自动 ===");
    let result = Query::new("users")
        .where_in("id", vec![1, 3, 5]) // Vec<i32> 自动转
        .query(&db)
        .select()
        .await?;
    println!("  IN (1,3,5) -> {} 条", result.rows.len());

    let result = Query::new("users")
        .where_between("score", 70.0, 92.0) // f64 区间自动转
        .where_eq("active", 1i32)
        .query(&db)
        .select()
        .await?;
    println!("  score BETWEEN 70 AND 92 AND active -> {} 条", result.rows.len());

    println!("\n=== 3. 排序 + 分页 + 条件 ===");
    let result = Query::new("users")
        .where_gt("age", 0)
        .order_by_desc("score")
        .limit(2)
        .query(&db)
        .select()
        .await?;
    for row in &result.rows {
        println!(
            "  name={} score={}",
            row.get("name").and_then(|v| v.as_str()).unwrap_or(""),
            row.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0),
        );
    }

    println!("\n=== 4. AdvancedQuery 聚合 + HAVING ===");
    let (sql, params) = AdvancedQuery::new("users")
        .group_by(&["active"])
        .having(WhereCondition::Gt("COUNT(*)".to_string(), 1i64.into()))
        .build_select();
    let result = db.query(&sql, &params).await?;
    println!("  生成 SQL: {}", sql);
    for row in &result.rows {
        println!("  {:?}", row.values);
    }

    println!("\n✅ 查询 API 无需再写 SqlValue::Type(...)，直接书写原生值即可。");
    Ok(())
}
