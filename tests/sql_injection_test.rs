//! SQL 注入防护集成测试
//!
//! 覆盖三个层面：
//! 1. `sql_safety` 工具的公开 API（标识符校验、转义、危险模式检测）
//! 2. ORM 查询构建器（Query / QueryBuilder / AdvancedQuery）对注入 payload 的抵抗
//! 3. 模型 CRUD 与参数绑定生成 SQL 的安全性

use torm::db::db_types::{Row, SqlValue};
use torm::db::database::Database;
use torm::orm::advanced_query::AdvancedQuery;
use torm::orm::model::{Model, Timestamps};
use torm::orm::query::{Query, QueryBuilder};
use torm::utils::sql_safety::{
    contains_injection_pattern, escape_string, quote_identifier, validate_identifier,
    validate_qualified_identifier, SqlSanitizer,
};

// ------------------------------------------------------------------
// 1. sql_safety 工具公开 API 测试
// ------------------------------------------------------------------

#[test]
fn validate_identifier_rejects_all_injection_payloads() {
    let payloads = [
        "name; DROP TABLE users",
        "1 OR 1=1",
        "id' OR '1'='1",
        "col UNION SELECT * FROM admin",
        "a -- comment",
        "a/*comment*/",
        "`evil`",
        "1abc",
        "a b",
        "col\" INJECT",
        "",
        "  ",
    ];
    for payload in payloads {
        assert!(
            validate_identifier(payload).is_err(),
            "expected `{}` to be rejected",
            payload
        );
    }
}

#[test]
fn validate_identifier_accepts_legitimate_names() {
    let legit = ["user_name", "name", "_tmp", "table2", "created_at", "User$"];
    for name in legit {
        assert!(
            validate_identifier(name).is_ok(),
            "expected `{}` to be accepted",
            name
        );
    }
}

#[test]
fn validate_qualified_identifier_handles_table_column_and_agg() {
    // 合法限定名 / 聚合写法
    assert!(validate_qualified_identifier("users.id").is_ok());
    assert!(validate_qualified_identifier("COUNT(*)").is_ok());
    assert!(validate_qualified_identifier("orders.total").is_ok());

    // 危险输入仍被拒绝
    assert!(validate_qualified_identifier("id; DROP TABLE x").is_err());
    assert!(validate_qualified_identifier("col OR 1=1").is_err());
    assert!(validate_qualified_identifier("col' OR '1'='1").is_err());
}

#[test]
fn quote_identifier_escapes_reserved_words() {
    assert_eq!(quote_identifier("select"), Some("`select`".to_string()));
    assert_eq!(quote_identifier("order"), Some("`order`".to_string()));
    assert_eq!(quote_identifier("user_name"), Some("user_name".to_string()));
    assert_eq!(quote_identifier("id; DROP"), None);
}

#[test]
fn escape_string_properly_escapes_single_quotes() {
    assert_eq!(escape_string("O'Reilly"), "O''Reilly");
    assert_eq!(escape_string("It's a 'test'"), "It''s a ''test''");
    assert_eq!(escape_string("''"), "''''");
    assert_eq!(escape_string("plain"), "plain");
    assert_eq!(escape_string(""), "");
}

#[test]
fn contains_injection_pattern_detects_attacks_but_allows_normal_sql() {
    // 危险模式应被识别
    assert!(contains_injection_pattern("1 OR 1=1; DROP TABLE users").is_some());
    assert!(contains_injection_pattern("UNION SELECT * FROM admin").is_some());
    assert!(contains_injection_pattern("'; DROP TABLE users; --").is_some());
    assert!(contains_injection_pattern("x = 'a' OR 1=1 --").is_some());

    // 正常 SQL 不应误报
    assert!(contains_injection_pattern("SELECT * FROM users WHERE id = ?").is_none());
    assert!(contains_injection_pattern("SELECT name FROM users").is_none());
    // 字符串字面量里的关键字不应误报
    assert!(contains_injection_pattern("SELECT * FROM users WHERE name = 'select'").is_none());
    assert!(contains_injection_pattern("SELECT * FROM users WHERE note = 'It''s a drop test'").is_none());
}

#[test]
fn sql_sanitizer_convenience_api() {
    assert_eq!(SqlSanitizer::identifier("user_name"), "user_name");
    // 保留字自动引用
    assert_eq!(SqlSanitizer::identifier("select"), "`select`");
    // 非法输入回退为空串（安全失败）
    assert_eq!(SqlSanitizer::identifier("id; DROP TABLE x"), "");
    assert_eq!(SqlSanitizer::escape("O'Reilly"), "O''Reilly");
    assert_eq!(SqlSanitizer::quote("user_name"), Some("user_name".to_string()));
    assert!(SqlSanitizer::check("DROP TABLE users").is_some());
}

// ------------------------------------------------------------------
// 2. 查询构建器对注入 payload 的抵抗
// ------------------------------------------------------------------

#[test]
fn query_builder_neutralizes_malicious_column_names() {
    let payload = "name; DROP TABLE users";
    let builder = QueryBuilder::new("users").where_eq(payload, "x");
    let (sql, bindings) = builder.build();

    // 恶意列名被替换为空串，不会把 DROP 拼接进 SQL
    assert!(!sql.contains("DROP"), "generated SQL must not contain DROP, got: {sql}");
    assert_eq!(bindings.len(), 1);
}

#[test]
fn query_neutralizes_malicious_table_name() {
    let payload = "users; DROP TABLE users";
    let query = Query::new(payload);
    let (sql, _) = query.build().return_sql();
    assert!(!sql.contains("DROP"), "generated SQL must not contain DROP, got: {sql}");
}

#[test]
fn query_neutralizes_malicious_order_by_column() {
    let payload = "created_at; DROP TABLE users";
    let query = Query::new("users").order_by_desc(payload);
    let (sql, _) = query.build().return_sql();
    assert!(!sql.contains("DROP"), "generated SQL must not contain DROP, got: {sql}");
}

#[test]
fn advanced_query_neutralizes_malicious_identifiers() {
    let evil_table = "users; DROP TABLE users";
    let evil_col = "id; DROP TABLE users";
    let evil_alias = "u; DROP TABLE users";

    let q = AdvancedQuery::new(evil_table)
        .select(&[evil_col])
        .inner_join(evil_table, "x.id = y.id")
        .join_alias(torm::orm::advanced_query::JoinType::Left, "t", evil_alias, "t.id = x.id")
        .group_by(&[evil_col])
        .order_by_desc(evil_col);

    let (sql, _) = q.build_select();
    assert!(!sql.contains("DROP"), "generated SQL must not contain DROP, got: {sql}");
}

#[test]
fn advanced_query_accepts_table_aliases_without_breaking() {
    // 回归测试：JOIN 表名常携带别名（如 `roles r`、`user_roles ur`），
    // 这些是合法 SQL，不应被安全校验误伤导致查询 500。
    let q = AdvancedQuery::new("users u")
        .select(&["u.id", "r.name AS role_name", "ur.role_id"])
        .inner_join("user_roles ur", "ur.user_id = u.id")
        .inner_join("roles r", "r.id = ur.role_id")
        .where_eq("u.status", "active");

    let (sql, bindings) = q.build_select();

    // 别名应当原样保留在 SQL 中
    assert!(sql.contains("FROM users u"), "expected alias in FROM, got: {sql}");
    assert!(sql.contains("INNER JOIN user_roles ur"), "expected join alias, got: {sql}");
    assert!(sql.contains("INNER JOIN roles r"), "expected join alias, got: {sql}");
    assert!(sql.contains("r.name AS role_name"), "expected select alias, got: {sql}");
    // 值仍是参数化绑定
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0], SqlValue::String("active".to_string()));
}

#[test]
fn advanced_query_still_rejects_alias_with_injection() {
    // 即便带别名，表名中混入分号/危险关键字仍应被拒绝
    let evil = "roles r; DROP TABLE users";
    let q = AdvancedQuery::new("users").inner_join(evil, "x.id = y.id");
    let (sql, _) = q.build_select();
    assert!(!sql.contains("DROP"), "generated SQL must not contain DROP, got: {sql}");
}

#[test]
fn malicious_string_values_are_parameterized_not_inlined() {
    // 即使值中包含注入 payload，也会作为绑定参数传递，不会内联进 SQL
    let payload = "'; DROP TABLE users; --";
    let query = Query::new("users").where_eq("name", payload);

    let (sql, bindings) = query.build().return_sql();
    // SQL 文本保持参数化占位符
    assert_eq!(sql, "SELECT * FROM users WHERE name = ?");
    assert_eq!(bindings.len(), 1);
    // 绑定参数保留原始 payload（交给数据库参数化处理，安全）
    assert_eq!(bindings[0], SqlValue::String(payload.to_string()));
}

#[test]
fn query_builder_uses_parameterized_bindings_for_values() {
    let payload = "x' OR '1'='1";
    let builder = QueryBuilder::new("users")
        .where_eq("email", payload)
        .where_like("name", "%' OR 1=1--");
    let (sql, bindings) = builder.build();

    assert!(!sql.contains(payload), "payload must not appear in SQL text");
    assert_eq!(bindings.len(), 2);
}

// ------------------------------------------------------------------
// 3. 模型 CRUD / 参数绑定安全（真实 SQLite 执行）
// ------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Account {
    id: String,
    name: String,
    timestamps: Timestamps,
}

impl Account {
    fn new(name: &str) -> Self {
        Self {
            id: format!("acc-{}", name),
            name: name.to_string(),
            timestamps: Timestamps::new(),
        }
    }
}

impl Model for Account {
    fn table_name() -> &'static str {
        "accounts"
    }
    fn id(&self) -> Option<String> {
        Some(self.id.clone())
    }
    fn set_id(&mut self, id: String) {
        self.id = id;
    }
    fn created_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.timestamps.created_at
    }
    fn updated_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.timestamps.updated_at
    }
    fn deleted_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.timestamps.deleted_at
    }
    fn set_created_at(&mut self, ts: chrono::DateTime<chrono::Utc>) {
        self.timestamps.created_at = Some(ts);
    }
    fn set_updated_at(&mut self, ts: chrono::DateTime<chrono::Utc>) {
        self.timestamps.updated_at = Some(ts);
    }
    fn set_deleted_at(&mut self, ts: Option<chrono::DateTime<chrono::Utc>>) {
        self.timestamps.deleted_at = ts;
    }
    fn columns(&self) -> Vec<(&'static str, SqlValue)> {
        vec![("name", SqlValue::String(self.name.clone()))]
    }
    fn from_row(row: &Row) -> Option<Self> {
        Some(Self {
            id: row.get("id")?.as_str()?.to_string(),
            name: row.get("name")?.as_str()?.to_string(),
            timestamps: Timestamps::new(),
        })
    }
}

#[tokio::test]
async fn model_crud_survives_injection_payload_in_value() {
    let db = Database::sqlite(":memory:").await.unwrap();
    db.execute(
        "CREATE TABLE accounts (id TEXT PRIMARY KEY, name TEXT, created_at TEXT, updated_at TEXT)",
        &[],
    )
    .await
    .unwrap();

    // 在值中注入 payload：应作为参数安全写入，而不是破坏 SQL
    let evil = "Alice'; DROP TABLE accounts; --";
    let mut account = Account::new(evil);
    db.create(&mut account).await.unwrap();

    // 表仍然存在，且能按主键精确查出该记录
    let found: Option<Account> = db.first_model(&account.id).await.unwrap();
    assert!(found.is_some(), "injected value should be stored, not break the table");
    assert_eq!(found.unwrap().name, evil);

    // 完整记录数应为 1（表未被 DROP）
    let all = db.find_models::<Account>().await.unwrap();
    assert_eq!(all.len(), 1, "table must not be dropped / truncated");
}

#[tokio::test]
async fn model_update_delete_survive_injection_payload_in_value() {
    let db = Database::sqlite(":memory:").await.unwrap();
    db.execute(
        "CREATE TABLE accounts (id TEXT PRIMARY KEY, name TEXT, created_at TEXT, updated_at TEXT)",
        &[],
    )
    .await
    .unwrap();

    let mut account = Account::new("safe");
    db.create(&mut account).await.unwrap();

    // UPDATE 值包含注入 payload
    let evil = "malicious' OR '1'='1";
    db.update(&mut account, &[("name", SqlValue::String(evil.to_string()))])
        .await
        .unwrap();
    let updated: Account = db.first_model(&account.id).await.unwrap().unwrap();
    assert_eq!(updated.name, evil);

    // DELETE 安全执行
    let affected = db.delete(&mut account).await.unwrap();
    assert_eq!(affected, 1);
    assert!(db.first_model::<Account>(&account.id).await.unwrap().is_none());
}

#[tokio::test]
async fn injection_payload_in_where_value_does_not_match_other_rows() {
    let db = Database::sqlite(":memory:").await.unwrap();
    db.execute(
        "CREATE TABLE accounts (id TEXT PRIMARY KEY, name TEXT, created_at TEXT, updated_at TEXT)",
        &[],
    )
    .await
    .unwrap();

    for name in ["alice", "bob", "charlie"] {
        let mut a = Account::new(name);
        db.create(&mut a).await.unwrap();
    }

    // 恶意 WHERE 值：参数化后应匹配 0 行，而不是利用 OR 1=1 命中全部
    let evil = "' OR '1'='1";
    let result = db
        .query(
            "SELECT * FROM accounts WHERE name = ?",
            &[SqlValue::String(evil.to_string())],
        )
        .await
        .unwrap();
    assert_eq!(result.rows.len(), 0, "parameterized OR-injection must match zero rows");
}
