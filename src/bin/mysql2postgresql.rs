// torm实现将mysql数据库中的数据迁移到postgresql数据库中
//
// 通过 torm 的 Database API 同时连接 MySQL（源）与 PostgreSQL（目标），
// 完成：
//   1. 从 MySQL information_schema 读取表结构与列定义
//   2. 将 MySQL 表结构映射为 PostgreSQL 的 CREATE TABLE
//   3. 分批读取 MySQL 数据并写入 PostgreSQL（支持事务）
//
// 用法：
//   mysql2postgresql --mhost <h> --mport <p> --mdb <db> --muser <u> --mpass <pw>
//                    --phost <h> --pport <p> --pdb <db> --puser <u> --ppass <pw>
//                    [--tables t1,t2] [--batch 1000] [--create-only] [--data-only]
//
// 不带任何参数时使用下面配置里的默认连接信息。

use std::collections::HashSet;
use torm::db::db_types::SqlValue;
use torm::db::database::{Database, DbError};

// ---------------------------------------------------------------------------
// 连接参数（可通过命令行覆盖）
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
struct Config {
    // MySQL 源库
    m_host: String,
    m_port: u16,
    m_db: String,
    m_user: String,
    m_pass: String,
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
            m_host: "localhost".to_string(),
            m_port: 3306,
            m_db: "mydb".to_string(),
            m_user: "root".to_string(),
            m_pass: "".to_string(),
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
// MySQL 列结构
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
struct MysqlColumn {
    name: String,
    data_type: String, // 基础类型，如 int、varchar
    is_nullable: bool,
    column_default: Option<String>,
    extra: String,      // auto_increment 等
    column_key: String, // PRI / UNI / MUL
}

impl MysqlColumn {
    fn is_auto_increment(&self) -> bool {
        self.extra.to_ascii_lowercase().contains("auto_increment")
    }
    fn is_primary(&self) -> bool {
        self.column_key.eq_ignore_ascii_case("PRI")
    }
    fn is_unique(&self) -> bool {
        self.column_key.eq_ignore_ascii_case("UNI")
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
            "--mhost" => { seen.insert("mhost".into()); if let Some(v) = take(&mut i) { cfg.m_host = v; } },
            "--mport" => { seen.insert("mport".into()); if let Some(v) = take(&mut i) { cfg.m_port = v.parse().unwrap_or(cfg.m_port); } },
            "--mdb" => { seen.insert("mdb".into()); if let Some(v) = take(&mut i) { cfg.m_db = v; } },
            "--muser" => { seen.insert("muser".into()); if let Some(v) = take(&mut i) { cfg.m_user = v; } },
            "--mpass" => { seen.insert("mpass".into()); if let Some(v) = take(&mut i) { cfg.m_pass = v; } },
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
            _ => {
                eprintln!("未知参数: {}", flag);
                eprintln!("{}", USAGE);
                std::process::exit(1);
            }
        }
        i += 1;
    }

    // 校验必需连接参数，缺失时提示并给出帮助（避免用默认参数去连接）
    let required = ["mhost", "mport", "mdb", "muser", "phost", "pport", "pdb", "puser"];
    let missing: Vec<&str> = required.iter().copied().filter(|k| !seen.contains(*k)).collect();
    if !missing.is_empty() {
        eprintln!("缺少必要连接参数: {}", missing.join(", "));
        eprintln!("{}", USAGE);
        std::process::exit(1);
    }

    cfg
}

const USAGE: &str = r#"
用法:
  mysql2postgresql --mhost <host> --mport <port> --mdb <db> --muser <user> --mpass <pass>
                   --phost <host> --pport <port> --pdb <db> --puser <user> --ppass <pass>
                   [--tables t1,t2] [--batch 1000] [--create-only] [--data-only]

选项:
  --mhost/--mport/--mdb/--muser/--mpass   MySQL 源库连接
  --phost/--pport/--pdb/--puser/--ppass   PostgreSQL 目标库连接
  --tables <a,b,c>                        只迁移指定表（默认全部）
  --batch <n>                             每批迁移行数（默认 1000）
  --create-only                           只创建表结构，不迁移数据
  --data-only                             只迁移数据，跳过建表
"#;

// ---------------------------------------------------------------------------
// 主流程
// ---------------------------------------------------------------------------
#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let cfg = parse_args();
    println!(
        "MySQL {}:{} @ {}  →  PostgreSQL {}:{} @ {}",
        cfg.m_host, cfg.m_port, cfg.m_db, cfg.p_host, cfg.p_port, cfg.p_db
    );

    // 连接源库（MySQL）与目标库（PostgreSQL）
    println!("连接 MySQL...");
    let mysql = Database::mysql(&cfg.m_host, cfg.m_port, &cfg.m_db, &cfg.m_user, &cfg.m_pass).await?;
    mysql.ping().await?;
    println!("  ✅ MySQL 连接成功 ({})", mysql.db_type());

    println!("连接 PostgreSQL...");
    let pg = Database::postgresql(&cfg.p_host, cfg.p_port, &cfg.p_db, &cfg.p_user, &cfg.p_pass).await?;
    pg.ping().await?;
    println!("  ✅ PostgreSQL 连接成功 ({})", pg.db_type());

    // 获取需要迁移的表
    let tables = resolve_tables(&mysql, &cfg).await?;
    println!("\n待迁移表 ({}): {:?}\n", tables.len(), tables);

    for table in &tables {
        // 1. 读取 MySQL 表结构
        let columns = fetch_columns(&mysql, &cfg.m_db, table).await?;
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
        let migrated = migrate_table(&mysql, &pg, table, &columns, cfg.batch_size).await?;
        println!("  ✅ 完成迁移 {}：{} 行", table, migrated);
    }

    mysql.close().await?;
    pg.close().await?;
    println!("\n🎉 迁移完成!");
    Ok(())
}

// ---------------------------------------------------------------------------
// 获取需要迁移的表清单
// ---------------------------------------------------------------------------
async fn resolve_tables(mysql: &Database, cfg: &Config) -> std::result::Result<Vec<String>, DbError> {
    if let Some(tables) = &cfg.tables {
        return Ok(tables.clone());
    }
    // 查询 MySQL 中所有非系统表
    let sql = format!(
        "SELECT table_name FROM information_schema.tables \
         WHERE table_schema = '{}' AND table_type = 'BASE TABLE' ORDER BY table_name",
        cfg.m_db
    );
    let result = mysql.query(&sql, &[]).await?;
    let mut tables = Vec::new();
    for row in &result.rows {
        if let Some(SqlValue::String(name)) = row.get("table_name") {
            tables.push(name.clone());
        }
    }
    Ok(tables)
}

// ---------------------------------------------------------------------------
// 读取 MySQL 表结构
// ---------------------------------------------------------------------------
async fn fetch_columns(
    mysql: &Database,
    db_name: &str,
    table: &str,
) -> std::result::Result<Vec<MysqlColumn>, DbError> {
    let sql = format!(
        "SELECT column_name, column_type, data_type, is_nullable, \
                column_default, extra, column_key \
         FROM information_schema.columns \
         WHERE table_schema = '{}' AND table_name = '{}' \
         ORDER BY ordinal_position",
        db_name, table
    );
    let result = mysql.query(&sql, &[]).await?;
    let mut columns = Vec::new();
    for row in &result.rows {
        let get = |k: &str| -> Option<String> {
            row.get(k).and_then(|v| v.as_str()).map(|s| s.to_string())
        };
        columns.push(MysqlColumn {
            name: get("column_name").unwrap_or_default(),
            data_type: get("data_type").unwrap_or_default(),
            is_nullable: get("is_nullable").map(|s| s.eq_ignore_ascii_case("YES")).unwrap_or(true),
            column_default: get("column_default"),
            extra: get("extra").unwrap_or_default(),
            column_key: get("column_key").unwrap_or_default(),
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
    columns: &[MysqlColumn],
) -> std::result::Result<(), DbError> {
    // 先清理旧表，保证可重复执行（若需保留请去掉 DROP）
    let _ = pg.execute(&format!("DROP TABLE IF EXISTS \"{}\"", table), &[]).await;

    let mut defs: Vec<String> = Vec::new();
    let mut primary_keys: Vec<String> = Vec::new();

    for col in columns {
        // 自增列用 SERIAL / BIGSERIAL 作为完整类型（SERIAL 本身即 INTEGER + 序列），
        // 不能写成 "INTEGER SERIAL"，否则 PostgreSQL 报 syntax error。
        let mut def = if col.is_auto_increment() {
            let serial = if col.data_type.to_ascii_lowercase().contains("bigint") {
                "BIGSERIAL"
            } else {
                "SERIAL"
            };
            format!("  \"{}\" {}", col.name, serial)
        } else {
            format!("  \"{}\" {}", col.name, mysql_type_to_pg(&col.data_type))
        };

        if !col.is_nullable {
            def.push_str(" NOT NULL");
        }
        if let Some(default) = normalize_default(&col.column_default) {
            if !col.is_auto_increment() {
                def.push_str(&format!(" DEFAULT {}", default));
            }
        }
        if col.is_unique() {
            def.push_str(" UNIQUE");
        }
        if col.is_primary() {
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
    mysql: &Database,
    pg: &Database,
    table: &str,
    columns: &[MysqlColumn],
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

    let mysql_cols = cols_mysql(columns);
    // 必须加 ORDER BY 保证 LIMIT/OFFSET 分批读取的行序稳定，
    // 否则 MySQL 无排序时跨批次可能读到重复行，导致主键/唯一约束冲突。
    let order_by: String = if let Some(pk) = columns.iter().find(|c| c.is_primary()) {
        format!("`{}`", pk.name)
    } else {
        mysql_cols.clone()
    };
    let select_sql = format!("SELECT {} FROM `{}` ORDER BY {}", mysql_cols, table, order_by);

    let mut offset: i64 = 0;
    let mut total: u64 = 0;

    loop {
        // 1. 从 MySQL 分批读取
        let page_sql = format!("{} LIMIT {} OFFSET {}", select_sql, batch_size, offset);
        let result = mysql.query(&page_sql, &[]).await?;
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
// MySQL 列名列表（反引号包裹，用于 SELECT）
// ---------------------------------------------------------------------------
fn cols_mysql(columns: &[MysqlColumn]) -> String {
    columns
        .iter()
        .map(|c| format!("`{}`", c.name))
        .collect::<Vec<_>>()
        .join(", ")
}

// ---------------------------------------------------------------------------
// MySQL 类型 → PostgreSQL 类型
// ---------------------------------------------------------------------------
fn mysql_type_to_pg(data_type: &str) -> &'static str {
    let t = data_type.to_ascii_lowercase();
    let t = t.trim();
    match t {
        "tinyint" | "smallint" => "SMALLINT",
        "mediumint" | "int" | "integer" => "INTEGER",
        "bigint" => "BIGINT",
        "decimal" | "numeric" => "NUMERIC",
        "float" => "REAL",
        "double" | "real" => "DOUBLE PRECISION",
        "bit" => "BIT",
        "char" => "CHAR",
        "varchar" | "varbinary" => "VARCHAR",
        "tinytext" | "text" | "mediumtext" | "longtext" => "TEXT",
        "tinyblob" | "blob" | "mediumblob" | "longblob" | "binary" => "BYTEA",
        "date" => "DATE",
        "datetime" | "timestamp" => "TIMESTAMP",
        "time" => "TIME",
        "year" => "SMALLINT",
        "json" => "JSONB",
        "enum" | "set" => "TEXT",
        "bool" | "boolean" => "BOOLEAN",
        _ => "TEXT", // 兜底
    }
}

// ---------------------------------------------------------------------------
// 默认值规范化：把 MySQL 默认值转为 PostgreSQL 可识别的形式
// ---------------------------------------------------------------------------
fn normalize_default(default: &Option<String>) -> Option<String> {
    let d = default.as_ref()?.trim();
    if d.is_empty() {
        return None;
    }
    let lower = d.to_ascii_lowercase();
    // 函数类默认值
    if lower == "current_timestamp()"
        || lower == "current_timestamp"
        || lower == "now()"
        || lower == "current_date()"
    {
        return Some("CURRENT_TIMESTAMP".to_string());
    }
    // 数值 / 布尔 / NULL
    if d.eq_ignore_ascii_case("null") {
        return Some("NULL".to_string());
    }
    if d.parse::<f64>().is_ok() || d.eq_ignore_ascii_case("true") || d.eq_ignore_ascii_case("false") {
        return Some(d.to_string());
    }
    // 字符串字面量（MySQL 可能带 b'..' 或多余括号）
    let clean = d.trim_matches(|c| c == '\'' || c == '"' || c == '(' || c == ')');
    // 双引号包裹，避免保留字
    Some(format!("\"{}\"", clean))
}
