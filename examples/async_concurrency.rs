//! 演示 torm 的异步并发查询能力。
//!
//! 运行方式：`cargo run --example async_concurrency`
//!
//! 本示例展示：
//! 1. `Database`（基于 rusqlite）在多任务下并发执行查询与写入。
//! 2. `AsyncStorageEngine`（纯 Rust 内存引擎的异步封装）在并发下安全读写。
//! 3. 通过 `tokio::spawn` 并发执行多个异步任务。

use std::collections::HashMap;
use std::sync::Arc;
use torm::{
    AsyncStorageEngine, Database, Query, SqlValue, TableSchema,
    StorageColumnDefinition as ColumnDefinition, StorageColumnType as ColumnType, WhereClause,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 TORM - Async Concurrency Demo\n");

    // 1. 使用 Database（SQLite 后端）做并发查询
    println!("1) Database (SQLite) concurrent queries");
    println!("========================================");
    database_concurrency().await?;
    println!();

    // 2. 使用 AsyncStorageEngine（纯 Rust 内存引擎）做并发读写
    println!("2) AsyncStorageEngine concurrent read/write");
    println!("=============================================");
    async_storage_concurrency().await?;
    println!();

    println!("🎉 All concurrent demos completed successfully!");
    Ok(())
}

/// 使用 `Database`（SQLite 后端）演示并发查询。
async fn database_concurrency() -> Result<(), Box<dyn std::error::Error>> {
    let db = Arc::new(Database::sqlite(":memory:").await?);

    db.execute(
        "CREATE TABLE IF NOT EXISTS products (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT,
            price INTEGER
        )",
        &[],
    )
    .await?;

    // 预置数据
    for i in 0..100 {
        db.execute(
            "INSERT INTO products (name, price) VALUES (?, ?)",
            &[SqlValue::String(format!("product_{}", i)), SqlValue::I32(i)],
        )
        .await?;
    }

    // 并发执行多个查询任务
    let mut handles = Vec::new();
    for offset in (0..100).step_by(25) {
        let db = Arc::clone(&db);
        handles.push(tokio::spawn(async move {
            let sql = format!("SELECT * FROM products ORDER BY id LIMIT 25 OFFSET {}", offset);
            let result = db.query(&sql, &[]).await?;
            Ok::<usize, torm::db::database::DbError>(result.rows.len())
        }));
    }

    let mut total = 0usize;
    for h in handles {
        total += h.await??;
    }
    println!("  4 concurrent queries fetched {} rows in total.", total);

    // 使用 Query builder 并发查询
    let mut handles = Vec::new();
    for min_price in [10, 20, 30, 40] {
        let db = Arc::clone(&db);
        handles.push(tokio::spawn(async move {
            Query::new("products")
                .where_gt("price", SqlValue::I32(min_price))
                .query(&db)
                .count()
                .await
        }));
    }
    for (i, h) in handles.into_iter().enumerate() {
        let count = h.await??;
        let n = count
            .rows
            .first()
            .and_then(|r| r.get("COUNT(*)").or_else(|| r.get("count")))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        println!("  price > {}  -> {} products", [10, 20, 30, 40][i], n);
    }

    Ok(())
}

/// 使用 `AsyncStorageEngine`（纯 Rust 内存引擎）演示并发读写。
async fn async_storage_concurrency() -> Result<(), Box<dyn std::error::Error>> {
    let engine = Arc::new(AsyncStorageEngine::new());

    let schema = TableSchema {
        name: "scores".to_string(),
        columns: vec![
            ColumnDefinition {
                name: "id".to_string(),
                column_type: ColumnType::Integer,
                nullable: false,
                default: None,
                unique: true,
            },
            ColumnDefinition {
                name: "player".to_string(),
                column_type: ColumnType::Text,
                nullable: false,
                default: None,
                unique: false,
            },
            ColumnDefinition {
                name: "score".to_string(),
                column_type: ColumnType::Integer,
                nullable: true,
                default: None,
                unique: false,
            },
        ],
        primary_key: Some("id".to_string()),
    };
    engine.create_table(schema).await?;

    // 并发插入 50 名玩家
    let mut handles = Vec::new();
    for i in 0..50u32 {
        let engine = Arc::clone(&engine);
        handles.push(tokio::spawn(async move {
            engine
                .insert(
                    "scores",
                    vec![
                        SqlValue::I32(i as i32),
                        SqlValue::String(format!("player_{}", i)),
                        SqlValue::I32((i * 3 % 100) as i32),
                    ],
                )
                .await
        }));
    }
    for h in handles {
        h.await??;
    }

    // 并发读取 + 并发更新
    let mut handles = Vec::new();
    for i in 0..50u32 {
        let engine = Arc::clone(&engine);
        handles.push(tokio::spawn(async move {
            // 每个任务并发读一次总量，并对自己的行做更新
            let result = engine.select("scores", None, None, None, None).await?;
            let mut updates = HashMap::new();
            updates.insert("score".to_string(), SqlValue::I32(100 + i as i32));
            let affected = engine
                .update(
                    "scores",
                    updates,
                    Some(WhereClause::Eq(
                        "id".to_string(),
                        SqlValue::I32(i as i32),
                    )),
                )
                .await?;
            Ok::<(usize, u64), torm::db::storage::StorageError>((result.rows.len(), affected))
        }));
    }

    let mut total_reads = 0usize;
    let mut total_affected = 0u64;
    for h in handles {
        let (read, affected) = h.await??;
        total_reads += read;
        total_affected += affected;
    }
    println!(
        "  50 concurrent tasks: {} total reads, {} rows updated.",
        total_reads, total_affected
    );

    // 验证最终数据
    let result = engine.select("scores", None, None, None, None).await?;
    let total_score: i32 = result
        .rows
        .iter()
        .filter_map(|r| r.get("score").and_then(|v| v.as_i32()))
        .sum();
    println!(
        "  Final: {} players, total score = {}",
        result.rows.len(),
        total_score
    );

    Ok(())
}
