// torm实现将sqlite数据库中的数据迁移到postgresql数据库中
//
// 通过 torm 的 Database API 同时连接 SQLite（源）与 PostgreSQL（目标），
// 完成：
//   1. 从 SQLite sqlite_master + PRAGMA table_info 读取表结构与列定义
//   2. 将 SQLite 表结构映射为 PostgreSQL 的 CREATE TABLE
//   3. 分批读取 SQLite 数据并写入 PostgreSQL（支持事务）
//
// 用法：
//   sqlite2postgres <sqlite_file>
//                  [--phost <h> --pport <p> --pdb <db> --puser <u> --ppass <pw>]
//                  [--tables t1,t2] [--batch 1000] [--create-only] [--data-only]
//
// 位置参数 <sqlite_file> 为源 SQLite 数据库文件路径；PostgreSQL 连接信息可通过
// 命令行覆盖，缺省时使用默认配置。

use std::collections::HashSet;
use torm::db::db_types::SqlValue;
use torm::db::database::{Database, DbError};

// ---------------------------------------------------------------------------
// 连接参数（可通过命令行覆盖）
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
struct Config {
    // SQLite 源库
    sqlite_file: String,
    // PostgreSQL 目标库
    p_host: String,
    p_port: u16,
    p_db: String,
    p_user: String,
    p_pass: String,
    // 行为控制
    tables: Option<Vec<String>>, // None 表示迁移全部表
    batch_size: usize,           // 每批迁移的行数
    create_only: bool,           // 只建表不迁数据
    data_only: bool,             // 只迁数据不建表
}

impl Default for Config {
    fn default() -> Self {
        Self {
            sqlite_file: "data.db".to_string(),
            p_host: "localhost".to_string(),
            p_port: 5432,
            p_db: "mydb".to_string(),
            p_user: "postgres".to_string(),
            p_pass: "".to_string(),
            tables: None,
            batch_size: 1000,
            create_only: false,
            data_only: false,
        }
    }
}

// ---------------------------------------------------------------------------
// SQLite 列结构
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
struct SqliteColumn {
    name: String,
    decl_type: String, // 声明类型，如 INTEGER / TEXT / VARCHAR(255)
    is_not_null: bool,
    default_value: Option<String>,
    is_pk: bool, // 是否主键列
    is_rowid: bool, // INTEGER PRIMARY KEY 自增别名（rowid）
}

impl SqliteColumn {
    fn is_auto_increment(&self) -> bool {
        self.is_rowid
    }
}

// ---------------------------------------------------------------------------
// 命令行解析（极简实现，避免引入 clap 依赖）
// ---------------------------------------------------------------------------
fn parse_args() -> Config {
    let mut cfg = Config::default();
    let mut seen: HashSet<String> = HashSet::new();
    let args: Vec<String> = std::env::args().skip(1).collect();

    // 完全无参数时直接打印帮助，而不是用默认配置去连接
    if args.is_empty() {
        println!("{}", USAGE);
        std::process::exit(0);
    }

    let mut i = 0;
    while i < args.len() {
        let flag = args[i].clone();
        // 取值并前进下标；布尔开关类参数无值。
        let take = |idx: &mut usize| -> Option<String> {
            if *idx + 1 < args.len() {
                *idx += 1;
                Some(args[*idx].clone())
            } else {
                None
            }
        };
        match flag.as_str() {
            // 位置参数：SQLite 文件路径
            "--phost" => { seen.insert("phost".into()); if let Some(v) = take(&mut i) { cfg.p_host = v; } },
            "--pport" => { seen.insert("pport".into()); if let Some(v) = take(&mut i) { cfg.p_port = v.parse().unwrap_or(cfg.p_port); } },
            "--pdb" => { seen.insert("pdb".into()); if let Some(v) = take(&mut i) { cfg.p_db = v; } },
            "--puser" => { seen.insert("puser".into()); if let Some(v) = take(&mut i) { cfg.p_user = v; } },
            "--ppass" => { seen.insert("ppass".into()); if let Some(v) = take(&mut i) { cfg.p_pass = v; } },
            "--tables" => if let Some(v) = take(&mut i) {
                cfg.tables = Some(v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect());
            },
            "--batch" => if let Some(v) = take(&mut i) { cfg.batch_size = v.parse().unwrap_or(cfg.batch_size); },
            "--create-only" => cfg.create_only = true,
            "--data-only" => cfg.data_only = true,
            "--help" | "-h" => {
                println!("{}", USAGE);
                std::process::exit(0);
            }
            // 未以 -- 开头的第一个参数视为 SQLite 文件
            _ if !flag.starts_with('-') && !seen.contains("sqlite_file") => {
                seen.insert("sqlite_file".into());
                cfg.sqlite_file = flag;
            }
            _ => {
                eprintln!("未知参数: {}", flag);
                eprintln!("{}", USAGE);
                std::process::exit(1);
            }
        }
        i += 1;
    }

    // 校验必需参数：SQLite 文件 + PostgreSQL 连接（缺失时给出帮助）
    let mut missing: Vec<&str> = Vec::new();
    if !seen.contains("sqlite_file") {
        missing.push("sqlite_file");
    }
    for k in ["phost", "pport", "pdb", "puser"] {
        if !seen.contains(k) {
            missing.push(k);
        }
    }
    if !missing.is_empty() {
        eprintln!("缺少必要参数: {}", missing.join(", "));
        eprintln!("{}", USAGE);
        std::process::exit(1);
    }

    cfg
}

const USAGE: &str = r#"
用法:
  sqlite2postgres <sqlite_file>
                 [--phost <host> --pport <port> --pdb <db> --puser <user> --ppass <pass>]
                 [--tables t1,t2] [--batch 1000] [--create-only] [--data-only]

选项:
  <sqlite_file>                         源 SQLite 数据库文件路径（必填）
  --phost/--pport/--pdb/--puser/--ppass PostgreSQL 目标库连接（缺省用默认配置）
  --tables <a,b,c>                      只迁移指定表（默认全部）
  --batch <n>                           每批迁移行数（默认 1000）
  --create-only                         只创建表结构，不迁移数据
  --data-only                           只迁移数据，跳过建表
"#;

// ---------------------------------------------------------------------------
// 主流程
// ---------------------------------------------------------------------------
#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let cfg = parse_args();
    println!(
        "SQLite file: {}  →  PostgreSQL {}:{} @ {}",
        cfg.sqlite_file, cfg.p_host, cfg.p_port, cfg.p_db
    );

    // 连接源库（SQLite）与目标库（PostgreSQL）
    println!("连接 SQLite...");
    let sqlite = Database::sqlite(&cfg.sqlite_file).await?;
    sqlite.ping().await?;
    println!("  ✅ SQLite 连接成功 ({})", sqlite.db_type());

    println!("连接 PostgreSQL...");
    let pg = Database::postgresql(&cfg.p_host, cfg.p_port, &cfg.p_db, &cfg.p_user, &cfg.p_pass).await?;
    pg.ping().await?;
    println!("  ✅ PostgreSQL 连接成功 ({})", pg.db_type());

    // 获取需要迁移的表
    let tables = resolve_tables(&sqlite, &cfg).await?;
    println!("\n待迁移表 ({}): {:?}\n", tables.len(), tables);

    for table in &tables {
        // 1. 读取 SQLite 表结构
        let columns = fetch_columns(&sqlite, table).await?;
        if columns.is_empty() {
            eprintln!("  ⚠️  表 {} 无列定义，跳过", table);
            continue;
        }

        // 2. 建表（除非只迁移数据）
        if !cfg.data_only {
            match create_pg_table(&pg, table, &columns).await {
                Ok(_) => println!("  ✅ 已创建表结构: {}", table),
                Err(e) => {
                    eprintln!("  ❌ 建表失败 {}: {}", table, e);
                    continue;
                }
            }
        }

        // 3. 迁移数据（除非只建表）
        if cfg.create_only {
            continue;
        }
        let migrated = migrate_table(&sqlite, &pg, table, &columns, cfg.batch_size).await?;
        println!("  ✅ 完成迁移 {}：{} 行", table, migrated);
    }

    sqlite.close().await?;
    pg.close().await?;
    println!("\n🎉 迁移完成!");
    Ok(())
}

// ---------------------------------------------------------------------------
// 获取需要迁移的表清单（排除 SQLite 系统表）
// ---------------------------------------------------------------------------
async fn resolve_tables(sqlite: &Database, cfg: &Config) -> std::result::Result<Vec<String>, DbError> {
    if let Some(tables) = &cfg.tables {
        return Ok(tables.clone());
    }
    let result = sqlite
        .query("SELECT name AS table_name FROM sqlite_master WHERE type = 'table' ORDER BY name", &[])
        .await?;
    let mut tables = Vec::new();
    for row in &result.rows {
        if let Some(SqlValue::String(name)) = row.get("table_name") {
            // 跳过 SQLite 内部表
            if !name.starts_with("sqlite_") {
                tables.push(name.clone());
            }
        }
    }
    Ok(tables)
}

// ---------------------------------------------------------------------------
// 读取 SQLite 表结构（PRAGMA table_info）
// ---------------------------------------------------------------------------
async fn fetch_columns(sqlite: &Database, table: &str) -> std::result::Result<Vec<SqliteColumn>, DbError> {
    let pragma_sql = format!("PRAGMA table_info(`{}`)", table);
    let result = sqlite.query(&pragma_sql, &[]).await?;
    let mut columns = Vec::new();
    for row in &result.rows {
        let get = |k: &str| -> Option<String> {
            row.get(k).and_then(|v| v.as_str()).map(|s| s.to_string())
        };
        let name = get("name").unwrap_or_default();
        let decl_type = get("type").unwrap_or_default();
        let is_pk = row
            .get("pk")
            .and_then(|v| v.as_i64())
            .map(|v| v > 0)
            .unwrap_or(false);
        let not_null = row
            .get("notnull")
            .and_then(|v| v.as_i64())
            .map(|v| v == 1)
            .unwrap_or(false);

        // INTEGER PRIMARY KEY（且声明类型不纯为 TEXT 等）视为 rowid 自增别名
        let decl_upper = decl_type.to_ascii_uppercase();
        let is_rowid = is_pk && decl_upper.contains("INT");

        columns.push(SqliteColumn {
            name,
            decl_type,
            is_not_null: not_null,
            default_value: get("dflt_value"),
            is_pk,
            is_rowid,
        });
    }
    Ok(columns)
}

// ---------------------------------------------------------------------------
// 在 PostgreSQL 创建表
// ---------------------------------------------------------------------------
async fn create_pg_table(
    pg: &Database,
    table: &str,
    columns: &[SqliteColumn],
) -> std::result::Result<(), DbError> {
    // 先清理旧表，保证可重复执行（若需保留请去掉 DROP）
    let _ = pg.execute(&format!("DROP TABLE IF EXISTS \"{}\"", table), &[]).await;

    let mut defs: Vec<String> = Vec::new();
    let mut primary_keys: Vec<String> = Vec::new();

    for col in columns {
        let mut def = format!("  \"{}\" {}", col.name, sqlite_type_to_pg(&col.decl_type));
        // INTEGER PRIMARY KEY → 用 SERIAL 让 PostgreSQL 自动生成自增值
        if col.is_auto_increment() {
            def = format!("  \"{}\" SERIAL", col.name);
        }
        if col.is_not_null && !col.is_auto_increment() {
            def.push_str(" NOT NULL");
        }
        if let Some(default) = normalize_default(&col.default_value) {
            if !col.is_auto_increment() {
                def.push_str(&format!(" DEFAULT {}", default));
            }
        }
        if col.is_pk {
            primary_keys.push(format!("\"{}\"", col.name));
        }
        defs.push(def);
    }

    if !primary_keys.is_empty() {
        defs.push(format!("  PRIMARY KEY ({})", primary_keys.join(", ")));
    }

    let sql = format!("CREATE TABLE IF NOT EXISTS \"{}\" (\n{}\n)", table, defs.join(",\n"));
    pg.execute(&sql, &[]).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 迁移单张表的数据（分批 + 事务）
// ---------------------------------------------------------------------------
async fn migrate_table(
    sqlite: &Database,
    pg: &Database,
    table: &str,
    columns: &[SqliteColumn],
    batch_size: usize,
) -> std::result::Result<u64, DbError> {
    // PostgreSQL 需要带引号的安全标识符，避免关键字冲突
    let pg_cols: Vec<String> = columns.iter().map(|c| format!("\"{}\"", c.name)).collect();
    let placeholders: Vec<String> = (1..=columns.len()).map(|i| format!("${}", i)).collect();
    let insert_sql = format!(
        "INSERT INTO \"{}\" ({}) VALUES ({})",
        table,
        pg_cols.join(", "),
        placeholders.join(", ")
    );

    let sqlite_cols = cols_sqlite(columns);
    // 必须加 ORDER BY 保证 LIMIT/OFFSET 分批读取的行序稳定，
    // 否则 SQLite 无排序时跨批次可能读到重复行，导致主键/唯一约束冲突。
    let order_by: String = if let Some(pk) = columns.iter().find(|c| c.is_pk) {
        format!("`{}`", pk.name)
    } else {
        sqlite_cols.clone()
    };
    let select_sql = format!("SELECT {} FROM `{}` ORDER BY {}", sqlite_cols, table, order_by);

    let mut offset: i64 = 0;
    let mut total: u64 = 0;

    loop {
        // 1. 从 SQLite 分批读取
        let page_sql = format!("{} LIMIT {} OFFSET {}", select_sql, batch_size, offset);
        let result = sqlite.query(&page_sql, &[]).await?;
        if result.rows.is_empty() {
            break;
        }

        // 2. 写入 PostgreSQL（单事务）
        let mut tx = pg.begin_transaction().await?;
        for row in &result.rows {
            let mut params: Vec<SqlValue> = Vec::with_capacity(columns.len());
            for col in columns {
                params.push(row.get(&col.name).cloned().unwrap_or(SqlValue::Null));
            }
            tx.execute(&insert_sql, &params).await?;
        }
        tx.commit().await?;

        total += result.rows.len() as u64;
        println!("  · {}: 已迁移 {} 行", table, total);
        offset += result.rows.len() as i64;
    }

    Ok(total)
}

// ---------------------------------------------------------------------------
// SQLite 列名列表（反引号包裹，用于 SELECT）
// ---------------------------------------------------------------------------
fn cols_sqlite(columns: &[SqliteColumn]) -> String {
    columns
        .iter()
        .map(|c| format!("`{}`", c.name))
        .collect::<Vec<_>>()
        .join(", ")
}

// ---------------------------------------------------------------------------
// SQLite 类型 → PostgreSQL 类型
// ---------------------------------------------------------------------------
fn sqlite_type_to_pg(decl_type: &str) -> &'static str {
    let t = decl_type.trim().to_ascii_uppercase();
    // SQLite 采用"类型亲和性"，按前缀/包含判断
    if t.contains("INT") {
        "BIGINT" // 含 INTEGER，统一用 BIGINT 兼容
    } else if t.contains("CHAR") || t.contains("CLOB") || t.contains("TEXT") {
        "TEXT"
    } else if t.contains("BLOB") || t.is_empty() {
        "BYTEA" // 无类型或 BLOB
    } else if t.contains("REAL") || t.contains("FLOA") || t.contains("DOUB") {
        "DOUBLE PRECISION"
    } else if t.contains("NUMERIC") || t.contains("DECIMAL") {
        "NUMERIC"
    } else if t.contains("DATE") {
        "DATE"
    } else if t.contains("TIME") {
        "TIMESTAMP"
    } else if t.contains("BOOL") {
        "BOOLEAN"
    } else {
        "TEXT" // 兜底
    }
}

// ---------------------------------------------------------------------------
// 默认值规范化：把 SQLite 默认值转为 PostgreSQL 可识别的形式
// ---------------------------------------------------------------------------
fn normalize_default(default: &Option<String>) -> Option<String> {
    let d = default.as_ref()?.trim();
    if d.is_empty() {
        return None;
    }
    let lower = d.to_ascii_lowercase();
    // SQLite 时间函数默认值
    if lower == "current_timestamp"
        || lower == "current_timestamp()"
        || lower == "datetime('now')"
        || lower == "strftime('%s','now')"
    {
        return Some("CURRENT_TIMESTAMP".to_string());
    }
    if lower == "current_date" || lower == "current_date()" {
        return Some("CURRENT_DATE".to_string());
    }
    if lower == "current_time" || lower == "current_time()" {
        return Some("CURRENT_TIME".to_string());
    }
    if lower == "true" {
        return Some("TRUE".to_string());
    }
    if lower == "false" {
        return Some("FALSE".to_string());
    }
    if d.eq_ignore_ascii_case("null") {
        return Some("NULL".to_string());
    }
    // 数值
    if d.parse::<f64>().is_ok() {
        return Some(d.to_string());
    }
    // 字符串字面量（SQLite 默认值一般直接是带引号的字符串）
    let clean = d.trim_matches(|c| c == '\'' || c == '"');
    Some(format!("'{}'", clean))
}
