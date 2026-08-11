// torm实现将postgresql数据库中的数据迁移到mysql数据库中
//
// 通过 torm 的 Database API 同时连接 PostgreSQL（源）与 MySQL（目标），
// 完成：
//   1. 从 PostgreSQL information_schema 读取表结构与列定义
//   2. 将 PostgreSQL 表结构映射为 MySQL 的 CREATE TABLE
//   3. 分批读取 PostgreSQL 数据并写入 MySQL（支持事务）
//
// 用法：
//   postgresql2mysql --phost <h> --pport <p> --pdb <db> --puser <u> --ppass <pw>
//                    --mhost <h> --mport <p> --mdb <db> --muser <u> --mpass <pw>
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
    // PostgreSQL 源库
    p_host: String,
    p_port: u16,
    p_db: String,
    p_user: String,
    p_pass: String,
    // MySQL 目标库
    m_host: String,
    m_port: u16,
    m_db: String,
    m_user: String,
    m_pass: String,
    // 行为控制
    tables: Option<Vec<String>>, // None 表示迁移全部表
    batch_size: usize,           // 每批迁移的行数
    create_only: bool,           // 只建表不迁数据
    data_only: bool,             // 只迁数据不建表
}

impl Default for Config {
    fn default() -> Self {
        Self {
            p_host: "localhost".to_string(),
            p_port: 5432,
            p_db: "mydb".to_string(),
            p_user: "postgres".to_string(),
            p_pass: "".to_string(),
            m_host: "localhost".to_string(),
            m_port: 3306,
            m_db: "mydb".to_string(),
            m_user: "root".to_string(),
            m_pass: "".to_string(),
            tables: None,
            batch_size: 1000,
            create_only: false,
            data_only: false,
        }
    }
}

// ---------------------------------------------------------------------------
// PostgreSQL 列结构
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
struct PgColumn {
    name: String,
    data_type: String,   // 基础类型，如 integer、character varying、timestamp without time zone
    udt_name: String,    // 如 int4、varchar、timestamp
    is_nullable: bool,
    column_default: Option<String>,
    is_identity: bool,   // identity 或 serial 生成列
    is_primary: bool,
    is_unique: bool,
}

impl PgColumn {
    fn is_auto_increment(&self) -> bool {
        self.is_identity
            || self
                .column_default
                .as_deref()
                .map(|d| {
                    d.contains("nextval(")
                        || d.contains("nextval (")
                })
                .unwrap_or(false)
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
            "--phost" => { seen.insert("phost".into()); if let Some(v) = take(&mut i) { cfg.p_host = v; } },
            "--pport" => { seen.insert("pport".into()); if let Some(v) = take(&mut i) { cfg.p_port = v.parse().unwrap_or(cfg.p_port); } },
            "--pdb" => { seen.insert("pdb".into()); if let Some(v) = take(&mut i) { cfg.p_db = v; } },
            "--puser" => { seen.insert("puser".into()); if let Some(v) = take(&mut i) { cfg.p_user = v; } },
            "--ppass" => { seen.insert("ppass".into()); if let Some(v) = take(&mut i) { cfg.p_pass = v; } },
            "--mhost" => { seen.insert("mhost".into()); if let Some(v) = take(&mut i) { cfg.m_host = v; } },
            "--mport" => { seen.insert("mport".into()); if let Some(v) = take(&mut i) { cfg.m_port = v.parse().unwrap_or(cfg.m_port); } },
            "--mdb" => { seen.insert("mdb".into()); if let Some(v) = take(&mut i) { cfg.m_db = v; } },
            "--muser" => { seen.insert("muser".into()); if let Some(v) = take(&mut i) { cfg.m_user = v; } },
            "--mpass" => { seen.insert("mpass".into()); if let Some(v) = take(&mut i) { cfg.m_pass = v; } },
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
    let required = ["phost", "pport", "pdb", "puser", "mhost", "mport", "mdb", "muser"];
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
  postgresql2mysql --phost <host> --pport <port> --pdb <db> --puser <user> --ppass <pass>
                   --mhost <host> --mport <port> --mdb <db> --muser <user> --mpass <pass>
                   [--tables t1,t2] [--batch 1000] [--create-only] [--data-only]

选项:
  --phost/--pport/--pdb/--puser/--ppass   PostgreSQL 源库连接
  --mhost/--mport/--mdb/--muser/--mpass   MySQL 目标库连接
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
        "PostgreSQL {}:{} @ {}  →  MySQL {}:{} @ {}",
        cfg.p_host, cfg.p_port, cfg.p_db, cfg.m_host, cfg.m_port, cfg.m_db
    );

    // 连接源库（PostgreSQL）与目标库（MySQL）
    println!("连接 PostgreSQL...");
    let pg = Database::postgresql(&cfg.p_host, cfg.p_port, &cfg.p_db, &cfg.p_user, &cfg.p_pass).await?;
    pg.ping().await?;
    println!("  ✅ PostgreSQL 连接成功 ({})", pg.db_type());

    println!("连接 MySQL...");
    let mysql = Database::mysql(&cfg.m_host, cfg.m_port, &cfg.m_db, &cfg.m_user, &cfg.m_pass).await?;
    mysql.ping().await?;
    println!("  ✅ MySQL 连接成功 ({})", mysql.db_type());

    // 获取需要迁移的表
    let tables = resolve_tables(&pg, &cfg).await?;
    println!("\n待迁移表 ({}): {:?}\n", tables.len(), tables);

    for table in &tables {
        // 1. 读取 PostgreSQL 表结构
        let columns = fetch_columns(&pg, &cfg.p_db, table).await?;
        if columns.is_empty() {
            eprintln!("  ⚠️  表 {} 无列定义，跳过", table);
            continue;
        }

        // 2. 建表（除非只迁移数据）
        if !cfg.data_only {
            match create_mysql_table(&mysql, table, &columns).await {
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
        let migrated = migrate_table(&pg, &mysql, table, &columns, cfg.batch_size).await?;
        println!("  ✅ 完成迁移 {}：{} 行", table, migrated);
    }

    pg.close().await?;
    mysql.close().await?;
    println!("\n🎉 迁移完成!");
    Ok(())
}

// ---------------------------------------------------------------------------
// 获取需要迁移的表清单
// ---------------------------------------------------------------------------
async fn resolve_tables(pg: &Database, cfg: &Config) -> std::result::Result<Vec<String>, DbError> {
    if let Some(tables) = &cfg.tables {
        return Ok(tables.clone());
    }
    // 查询 PostgreSQL 当前 schema 下所有普通表
    let sql = format!(
        "SELECT tablename AS table_name \
         FROM pg_tables \
         WHERE schemaname = 'public' \
         ORDER BY tablename"
    );
    let result = pg.query(&sql, &[]).await?;
    let mut tables = Vec::new();
    for row in &result.rows {
        if let Some(SqlValue::String(name)) = row.get("table_name") {
            tables.push(name.clone());
        }
    }
    Ok(tables)
}

// ---------------------------------------------------------------------------
// 读取 PostgreSQL 表结构
// ---------------------------------------------------------------------------
async fn fetch_columns(
    pg: &Database,
    db_name: &str,
    table: &str,
) -> std::result::Result<Vec<PgColumn>, DbError> {
    // 从 information_schema 读列定义
    let sql = format!(
        "SELECT c.column_name, c.data_type, c.udt_name, c.is_nullable, c.column_default \
         FROM information_schema.columns c \
         WHERE c.table_schema = '{}' AND c.table_name = '{}' \
         ORDER BY c.ordinal_position",
        db_name, table
    );
    let result = pg.query(&sql, &[]).await?;
    let mut columns: Vec<PgColumn> = Vec::new();
    for row in &result.rows {
        let get = |k: &str| -> Option<String> {
            row.get(k).and_then(|v| v.as_str()).map(|s| s.to_string())
        };
        columns.push(PgColumn {
            name: get("column_name").unwrap_or_default(),
            data_type: get("data_type").unwrap_or_default(),
            udt_name: get("udt_name").unwrap_or_default(),
            is_nullable: get("is_nullable").map(|s| s.eq_ignore_ascii_case("YES")).unwrap_or(true),
            column_default: get("column_default"),
            is_identity: false,
            is_primary: false,
            is_unique: false,
        });
    }

    // 标注自增/identity 列（列默认值含 nextval 即 serial/bigserial）
    for col in &mut columns {
        col.is_identity = col
            .column_default
            .as_deref()
            .map(|d| d.contains("nextval(") || d.contains("nextval ("))
            .unwrap_or(false);
    }

    // 主键约束
    let pk_sql = format!(
        "SELECT kcu.column_name \
         FROM information_schema.table_constraints tc \
         JOIN information_schema.key_column_usage kcu \
           ON tc.constraint_name = kcu.constraint_name \
          AND tc.table_schema = kcu.table_schema \
         WHERE tc.constraint_type = 'PRIMARY KEY' \
           AND tc.table_schema = '{}' AND tc.table_name = '{}'",
        db_name, table
    );
    let pk_result = pg.query(&pk_sql, &[]).await?;
    let pk_names: Vec<String> = pk_result
        .rows
        .iter()
        .filter_map(|row| row.get("column_name").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .collect();
    for col in &mut columns {
        col.is_primary = pk_names.contains(&col.name);
    }

    // 唯一约束（非主键）
    let uq_sql = format!(
        "SELECT kcu.column_name \
         FROM information_schema.table_constraints tc \
         JOIN information_schema.key_column_usage kcu \
           ON tc.constraint_name = kcu.constraint_name \
          AND tc.table_schema = kcu.table_schema \
         WHERE tc.constraint_type = 'UNIQUE' \
           AND tc.table_schema = '{}' AND tc.table_name = '{}'",
        db_name, table
    );
    let uq_result = pg.query(&uq_sql, &[]).await?;
    let uq_names: Vec<String> = uq_result
        .rows
        .iter()
        .filter_map(|row| row.get("column_name").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .collect();
    for col in &mut columns {
        col.is_unique = !col.is_primary && uq_names.contains(&col.name);
    }

    Ok(columns)
}

// ---------------------------------------------------------------------------
// 在 MySQL 创建表
// ---------------------------------------------------------------------------
async fn create_mysql_table(
    mysql: &Database,
    table: &str,
    columns: &[PgColumn],
) -> std::result::Result<(), DbError> {
    // 先清理旧表，保证可重复执行（若需保留请去掉 DROP）
    let _ = mysql.execute(&format!("DROP TABLE IF EXISTS `{}`", table), &[]).await;

    let mut defs: Vec<String> = Vec::new();
    let mut primary_keys: Vec<String> = Vec::new();

    for col in columns {
        let mut def = format!("  `{}` {}", col.name, pg_type_to_mysql(&col.data_type, &col.udt_name));
        if col.is_auto_increment() {
            def.push_str(" AUTO_INCREMENT");
        }
        if !col.is_nullable {
            def.push_str(" NOT NULL");
        }
        if let Some(default) = normalize_default(&col.column_default) {
            if !col.is_auto_increment() {
                def.push_str(&format!(" DEFAULT {}", default));
            }
        }
        if col.is_unique {
            def.push_str(" UNIQUE");
        }
        if col.is_primary {
            primary_keys.push(format!("`{}`", col.name));
        }
        defs.push(def);
    }

    if !primary_keys.is_empty() {
        defs.push(format!("  PRIMARY KEY ({})", primary_keys.join(", ")));
    }

    // MySQL 默认使用 utf8mb4 + InnoDB
    let sql = format!(
        "CREATE TABLE IF NOT EXISTS `{}` (\n{}\n) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4",
        table,
        defs.join(",\n")
    );
    mysql.execute(&sql, &[]).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 迁移单张表的数据（分批 + 事务）
// ---------------------------------------------------------------------------
async fn migrate_table(
    pg: &Database,
    mysql: &Database,
    table: &str,
    columns: &[PgColumn],
    batch_size: usize,
) -> std::result::Result<u64, DbError> {
    // MySQL 需要反引号包裹的安全标识符，避免关键字冲突
    let mysql_cols: Vec<String> = columns.iter().map(|c| format!("`{}`", c.name)).collect();
    let placeholders: Vec<&str> = vec!["?"; columns.len()];
    let insert_sql = format!(
        "INSERT INTO `{}` ({}) VALUES ({})",
        table,
        mysql_cols.join(", "),
        placeholders.join(", ")
    );

    // PostgreSQL 列名（双引号包裹，兼容大写/关键字）
    let pg_cols: Vec<String> = columns.iter().map(|c| format!("\"{}\"", c.name)).collect();
    let select_sql = format!("SELECT {} FROM \"{}\"", pg_cols.join(", "), table);

    let mut offset: i64 = 0;
    let mut total: u64 = 0;

    loop {
        // 1. 从 PostgreSQL 分批读取
        let page_sql = format!("{} LIMIT {} OFFSET {}", select_sql, batch_size, offset);
        let result = pg.query(&page_sql, &[]).await?;
        if result.rows.is_empty() {
            break;
        }

        // 2. 写入 MySQL（单事务）
        let mut tx = mysql.begin_transaction().await?;
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
// PostgreSQL 类型 → MySQL 类型
// ---------------------------------------------------------------------------
fn pg_type_to_mysql(data_type: &str, udt_name: &str) -> &'static str {
    let t = data_type.to_ascii_lowercase();
    let u = udt_name.to_ascii_lowercase();
    let t = t.trim();
    let u = u.trim();
    match (t, u) {
        // 整数
        ("smallint", "int2") => "SMALLINT",
        ("integer", "int4") => "INT",
        ("bigint", "int8") => "BIGINT",
        // 浮点 / 数值
        ("real", "float4") => "FLOAT",
        ("double precision", "float8") => "DOUBLE",
        ("numeric", "numeric") => "DECIMAL",
        ("money", "money") => "DECIMAL(19,2)",
        // 布尔
        ("boolean", "bool") => "TINYINT(1)",
        // 字符
        ("character varying", "varchar") => "VARCHAR(255)",
        ("character", "bpchar") => "CHAR(1)",
        ("text", "text") => "TEXT",
        ("name", "name") => "VARCHAR(64)",
        ("citext", "citext") => "VARCHAR(255)",
        // 二进制
        ("bytea", "bytea") => "BLOB",
        ("bit", "bit") => "BIT",
        ("bit varying", "varbit") => "VARBINARY(255)",
        // 时间
        ("timestamp without time zone", "timestamp") => "DATETIME",
        ("timestamp with time zone", "timestamptz") => "DATETIME",
        ("date", "date") => "DATE",
        ("time without time zone", "time") => "TIME",
        ("time with time zone", "timetz") => "TIME",
        // 其他
        ("json", "json") => "JSON",
        ("jsonb", "jsonb") => "JSON",
        ("uuid", "uuid") => "CHAR(36)",
        ("inet", "inet") => "VARCHAR(45)",
        ("macaddr", "macaddr") => "VARCHAR(17)",
        ("interval", "interval") => "VARCHAR(64)",
        ("smallserial", "int2") => "SMALLINT",
        ("serial", "int4") => "INT",
        ("bigserial", "int8") => "BIGINT",
        ("array", _) => "JSON",
        _ => "TEXT", // 兜底
    }
}

// ---------------------------------------------------------------------------
// 默认值规范化：把 PostgreSQL 默认值转为 MySQL 可识别的形式
// ---------------------------------------------------------------------------
fn normalize_default(default: &Option<String>) -> Option<String> {
    let d = default.as_ref()?.trim();
    if d.is_empty() {
        return None;
    }
    let lower = d.to_ascii_lowercase();

    // 序列生成列：由 AUTO_INCREMENT 替代，跳过
    if lower.contains("nextval(") || lower.contains("nextval (") {
        return None;
    }
    // 函数类默认值
    if lower == "now()" || lower == "current_timestamp" || lower == "current_timestamp()" {
        return Some("CURRENT_TIMESTAMP".to_string());
    }
    if lower == "current_date" || lower == "current_date()" {
        return Some("CURRENT_DATE".to_string());
    }
    if lower == "current_user" {
        return Some("CURRENT_USER".to_string());
    }
    if lower == "true" {
        return Some("1".to_string());
    }
    if lower == "false" {
        return Some("0".to_string());
    }
    if d.eq_ignore_ascii_case("null") {
        return Some("NULL".to_string());
    }
    // 数值
    if d.parse::<f64>().is_ok() {
        return Some(d.to_string());
    }
    // PostgreSQL 字符串默认值形如 'xxx'::character varying，需要去掉类型转换后缀
    let cleaned = strip_pg_type_cast(d);
    Some(format!("'{}'", cleaned.trim_matches(|c| c == '\'' || c == '"')))
}

// 去掉 PostgreSQL 的 ::type 转换后缀，如 'abc'::character varying → 'abc'
fn strip_pg_type_cast(s: &str) -> String {
    if let Some(pos) = s.find("::") {
        let prefix = s[..pos].trim();
        prefix.to_string()
    } else {
        s.to_string()
    }
}
