//! Tests for the `#[derive(Model)]` macro.
//!
//! Verifies that the derive macro generates a working `Model` impl that
//! produces correct `columns()`, `from_row()`, primary-key accessors and
//! timestamp accessors.

use chrono::Utc;
use torm::db::db_types::{Row, SqlValue};
use torm::orm::model::Timestamps;
use torm::orm::query::Query;
use torm::Model;

#[derive(Debug, Clone, Model)]
#[model(table_name = "users")]
struct User {
    id: String,
    name: String,
    email: String,
    age: Option<i32>,
    status: String,
    #[model(column = "created_on")]
    created_on: chrono::DateTime<Utc>,
    timestamps: Timestamps,
    #[model(skip)]
    transient_cache: String,
}

#[test]
fn derive_generates_table_name() {
    assert_eq!(User::table_name(), "users");
}

#[test]
fn derive_generates_primary_key() {
    let mut user = User {
        id: "user_1".to_string(),
        name: "Alice".to_string(),
        email: "alice@example.com".to_string(),
        age: Some(30),
        status: "active".to_string(),
        created_on: Utc::now(),
        timestamps: Timestamps::new(),
        transient_cache: "ignored".to_string(),
    };

    assert_eq!(user.id(), Some("user_1".to_string()));
    user.set_id("user_2".to_string());
    assert_eq!(user.id, "user_2".to_string());
}

#[test]
fn derive_handles_empty_id_as_none() {
    let user = User {
        id: "".to_string(),
        name: "Bob".to_string(),
        email: "bob@example.com".to_string(),
        age: None,
        status: "active".to_string(),
        created_on: Utc::now(),
        timestamps: Timestamps::new(),
        transient_cache: String::new(),
    };
    assert_eq!(user.id(), None);
}

#[test]
fn derive_generates_timestamp_accessors() {
    let now = Utc::now();
    let mut user = User {
        id: "user_1".to_string(),
        name: "Alice".to_string(),
        email: "a@b.c".to_string(),
        age: None,
        status: "active".to_string(),
        created_on: now,
        timestamps: Timestamps::new(),
        transient_cache: String::new(),
    };

    assert_eq!(user.created_at(), None);
    user.set_created_at(now);
    assert_eq!(user.created_at(), Some(now));

    user.set_updated_at(now);
    assert_eq!(user.updated_at(), Some(now));

    user.set_deleted_at(Some(now));
    assert_eq!(user.deleted_at(), Some(now));
    assert!(user.is_deleted());
}

#[test]
fn derive_generates_columns() {
    let user = User {
        id: "user_1".to_string(),
        name: "Alice".to_string(),
        email: "alice@example.com".to_string(),
        age: Some(30),
        status: "active".to_string(),
        created_on: Utc::now(),
        timestamps: Timestamps::new(),
        transient_cache: "not persisted".to_string(),
    };

    let columns = user.columns();
    let map: std::collections::HashMap<&str, SqlValue> =
        columns.into_iter().collect();

    // Persistable fields present.
    assert_eq!(map.get("name"), Some(&SqlValue::String("Alice".to_string())));
    assert_eq!(map.get("email"), Some(&SqlValue::String("alice@example.com".to_string())));
    assert_eq!(map.get("age"), Some(&SqlValue::I32(30)));
    assert_eq!(map.get("status"), Some(&SqlValue::String("active".to_string())));

    // Column rename applied.
    assert!(map.contains_key("created_on"));

    // Skipped field must not appear.
    assert!(!map.contains_key("transient_cache"));
    // Timestamps field itself is not a persist column.
    assert!(!map.contains_key("timestamps"));
    // Primary key is not emitted by columns().
    assert!(!map.contains_key("id"));
}

#[test]
fn derive_columns_handle_option_null() {
    let user = User {
        id: "user_1".to_string(),
        name: "Bob".to_string(),
        email: "b@b.b".to_string(),
        age: None,
        status: "active".to_string(),
        created_on: Utc::now(),
        timestamps: Timestamps::new(),
        transient_cache: String::new(),
    };

    let columns = user.columns();
    let map: std::collections::HashMap<&str, SqlValue> =
        columns.into_iter().collect();
    assert_eq!(map.get("age"), Some(&SqlValue::Null));
}

// ------------------------------------------------------------------
// myworldquant-style model: i64 primary key + standalone timestamp fields
// + association fields that should be auto-skipped.
// ------------------------------------------------------------------
#[derive(Debug, Clone, Model)]
#[model(table_name = "users")]
struct UserRecord {
    id: i64,
    username: String,
    nickname: String,
    status: i32,
    created_at: Option<chrono::DateTime<Utc>>,
    updated_at: Option<chrono::DateTime<Utc>>,
    deleted_at: Option<chrono::DateTime<Utc>>,
    // Association / non-DB fields (auto-skipped).
    role_ids: Option<Vec<i64>>,
    role_names: Option<Vec<String>>,
}

#[test]
fn derive_i64_primary_key() {
    let mut u = UserRecord {
        id: 0,
        username: "alice".to_string(),
        nickname: "Alice".to_string(),
        status: 1,
        created_at: None,
        updated_at: None,
        deleted_at: None,
        role_ids: None,
        role_names: None,
    };
    assert_eq!(u.id(), None);

    u.set_id("42".to_string());
    assert_eq!(u.id, 42);
    assert_eq!(u.id(), Some("42".to_string()));

    // Invalid id falls back to 0.
    u.set_id("not-a-number".to_string());
    assert_eq!(u.id, 0);
    assert_eq!(u.id(), None);
}

#[test]
fn derive_standalone_timestamps() {
    let now = Utc::now();
    let mut u = UserRecord {
        id: 1,
        username: "alice".to_string(),
        nickname: "Alice".to_string(),
        status: 1,
        created_at: None,
        updated_at: None,
        deleted_at: None,
        role_ids: None,
        role_names: None,
    };

    assert_eq!(u.created_at(), None);
    u.set_created_at(now);
    assert_eq!(u.created_at(), Some(now));
    assert_eq!(u.created_at, Some(now));

    u.set_updated_at(now);
    assert_eq!(u.updated_at(), Some(now));

    u.set_deleted_at(Some(now));
    assert_eq!(u.deleted_at(), Some(now));
    assert!(u.is_deleted());
}

#[test]
fn derive_standalone_columns_exclude_timestamps_and_associations() {
    let u = UserRecord {
        id: 7,
        username: "alice".to_string(),
        nickname: "Alice".to_string(),
        status: 1,
        created_at: Some(Utc::now()),
        updated_at: Some(Utc::now()),
        deleted_at: None,
        role_ids: Some(vec![1, 2]),
        role_names: Some(vec!["admin".to_string()]),
    };

    let columns = u.columns();
    let map: std::collections::HashMap<&str, SqlValue> =
        columns.into_iter().collect();

    assert!(map.contains_key("username"));
    assert!(map.contains_key("nickname"));
    assert!(map.contains_key("status"));
    // Timestamps and associations are NOT persist columns.
    assert!(!map.contains_key("created_at"));
    assert!(!map.contains_key("updated_at"));
    assert!(!map.contains_key("deleted_at"));
    assert!(!map.contains_key("role_ids"));
    assert!(!map.contains_key("role_names"));
    assert!(!map.contains_key("id"));
}

#[test]
fn derive_standalone_from_row() {
    // Simulate SQLite: integers come back as I64, datetimes as TEXT strings.
    let now = Utc::now();
    let now_str = now.format("%Y-%m-%d %H:%M:%S").to_string();
    let row = Row::new(
        vec![
            "id".to_string(),
            "username".to_string(),
            "nickname".to_string(),
            "status".to_string(),
            "created_at".to_string(),
            "updated_at".to_string(),
            "deleted_at".to_string(),
        ],
        vec![
            SqlValue::I64(9),
            SqlValue::String("bob".to_string()),
            SqlValue::String("Bob".to_string()),
            SqlValue::I64(1), // SQLite returns INTEGER as I64
            SqlValue::String(now_str.clone()),
            SqlValue::String(now_str),
            SqlValue::Null,
        ],
    );

    let u = UserRecord::from_row(&row).expect("from_row should succeed");
    assert_eq!(u.id, 9);
    assert_eq!(u.username, "bob");
    assert_eq!(u.status, 1);
    assert_eq!(u.created_at().unwrap().timestamp(), now.timestamp());
    assert_eq!(u.deleted_at(), None);
    // Association fields default.
    assert_eq!(u.role_ids, None);
    assert_eq!(u.role_names, None);
}

#[test]
fn derive_from_row_round_trip() {
    let now = Utc::now();
    let row = Row::new(
        vec![
            "id".to_string(),
            "name".to_string(),
            "email".to_string(),
            "age".to_string(),
            "status".to_string(),
            "created_on".to_string(),
            "created_at".to_string(),
            "updated_at".to_string(),
            "deleted_at".to_string(),
        ],
        vec![
            SqlValue::String("user_1".to_string()),
            SqlValue::String("Alice".to_string()),
            SqlValue::String("alice@example.com".to_string()),
            SqlValue::I32(30),
            SqlValue::String("active".to_string()),
            SqlValue::DateTime(now),
            SqlValue::DateTime(now),
            SqlValue::DateTime(now),
            SqlValue::Null,
        ],
    );

    let user = User::from_row(&row).expect("from_row should succeed");

    assert_eq!(user.id, "user_1");
    assert_eq!(user.name, "Alice");
    assert_eq!(user.email, "alice@example.com");
    assert_eq!(user.age, Some(30));
    assert_eq!(user.status, "active");
    assert_eq!(user.created_on, now);
    // Timestamps populated from row.
    assert_eq!(user.created_at(), Some(now));
    assert_eq!(user.updated_at(), Some(now));
    assert_eq!(user.deleted_at(), None);
    // Skipped field defaults.
    assert_eq!(user.transient_cache, "");
}

// ------------------------------------------------------------------
// End-to-end test: verify a #[derive(Model)] struct works through the
// real SQLite database (create -> find -> update -> delete).
// ------------------------------------------------------------------
#[derive(Debug, Clone, Model)]
#[model(table_name = "people")]
struct Person {
    id: i64,
    name: String,
    age: Option<i32>,
    email: String,
    created_at: Option<chrono::DateTime<Utc>>,
    updated_at: Option<chrono::DateTime<Utc>>,
    deleted_at: Option<chrono::DateTime<Utc>>,
    // Association field (auto-skipped).
    tags: Option<Vec<String>>,
}

#[tokio::test]
async fn derive_model_end_to_end_sqlite() {
    let db = torm::Database::sqlite(":memory:").await.unwrap();
    db.execute(
        "CREATE TABLE people (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT,
            age INTEGER,
            email TEXT,
            created_at TEXT,
            updated_at TEXT,
            deleted_at TEXT
        )",
        &[],
    )
    .await
    .unwrap();

    // Create via ORM (uses derive columns() + hooks for timestamps).
    let mut p = Person {
        id: 0,
        name: "Alice".to_string(),
        age: Some(30),
        email: "alice@example.com".to_string(),
        created_at: None,
        updated_at: None,
        deleted_at: None,
        tags: None,
    };
    db.create_model(&mut p).await.unwrap();
    assert!(p.id > 0, "auto-increment id should be set");
    assert!(p.created_at.is_some(), "created_at set by before_create hook");

    // Find back via ORM (uses derive from_row()).
    let found: Option<Person> = db.first_model(&p.id.to_string()).await.unwrap();
    let found = found.expect("person should be found");
    assert_eq!(found.name, "Alice");
    assert_eq!(found.age, Some(30));
    assert_eq!(found.email, "alice@example.com");
    assert_eq!(found.created_at.is_some(), true);
    // Association field defaulted.
    assert_eq!(found.tags, None);

    // Update via ORM.
    let mut found = found;
    db.update_model(&mut found, &[("age", SqlValue::I32(31))])
        .await
        .unwrap();
    assert!(found.updated_at.is_some(), "updated_at set by before_update hook");
    let reloaded: Option<Person> = db.first_model(&p.id.to_string()).await.unwrap();
    assert_eq!(reloaded.unwrap().age, Some(31));

    // Delete via ORM.
    let mut to_delete = Person {
        id: p.id,
        name: String::new(),
        age: None,
        email: String::new(),
        created_at: None,
        updated_at: None,
        deleted_at: None,
        tags: None,
    };
    db.delete_model(&mut to_delete).await.unwrap();
    let gone: Option<Person> = db.first_model(&p.id.to_string()).await.unwrap();
    assert!(gone.is_none());
}

// ------------------------------------------------------------------
// SqlStatement: Query 构建后可直接 execute / query / return_sql。
// ------------------------------------------------------------------
#[tokio::test]
async fn sql_statement_execute_and_query() {
    let db = torm::Database::sqlite(":memory:").await.unwrap();
    db.execute(
        "CREATE TABLE users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT,
            age INTEGER
        )",
        &[],
    )
    .await
    .unwrap();

    // INSERT 直接执行（Query::insert(columns, db)）
    let affected = Query::new("users")
        .insert(
            &[
                ("name", SqlValue::String("Alice".to_string())),
                ("age", SqlValue::I32(30)),
            ],
            &db,
        )
        .await
        .unwrap();
    assert_eq!(affected, 1);

    Query::new("users")
        .insert(
            &[
                ("name", SqlValue::String("Bob".to_string())),
                ("age", SqlValue::I32(25)),
            ],
            &db,
        )
        .await
        .unwrap();

    // UPDATE 直接执行（Query::update(updates, db)），并通过 return_sql() 查看 SQL
    let q = Query::new("users").where_eq("name", SqlValue::String("Alice".to_string()));
    let affected = q
        .update(
            &{
                let mut m = std::collections::HashMap::new();
                m.insert("age".to_string(), SqlValue::I32(31));
                m
            },
            &db,
        )
        .await
        .unwrap();
    assert_eq!(affected, 1);

    // return_sql() 查看最近一次操作（UPDATE）的 SQL 与参数
    let (sql, params) = q.return_sql();
    assert!(sql.starts_with("UPDATE users SET"));
    assert_eq!(params, vec![
        SqlValue::I32(31),
        SqlValue::String("Alice".to_string())
    ]);

    // SELECT 直接查询（Query::build().query()）
    let r = Query::new("users")
        .where_eq("age", SqlValue::I32(31))
        .build()
        .query(&db)
        .await
        .unwrap();
    assert_eq!(r.rows.len(), 1);

    // COUNT 直接查询
    let r = Query::new("users").count().query(&db).await.unwrap();
    let total = r
        .rows
        .first()
        .and_then(|row| row.get("COUNT(*)"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    assert_eq!(total, 2);

    // QueryExecutor: q.query(db).count() / q.query(db).select()
    let r = Query::new("users").query(&db).count().await.unwrap();
    let total = r
        .rows
        .first()
        .and_then(|row| row.get("COUNT(*)"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    assert_eq!(total, 2);

    let r = Query::new("users")
        .where_eq("age", SqlValue::I32(31))
        .query(&db)
        .select()
        .await
        .unwrap();
    assert_eq!(r.rows.len(), 1);

    // DELETE 直接执行（Query::delete(db)）
    let affected = Query::new("users")
        .where_eq("name", SqlValue::String("Bob".to_string()))
        .delete(&db)
        .await
        .unwrap();
    assert_eq!(affected, 1);

    // build() 后 return_sql() 查看 SELECT SQL
    let (sql, params) = Query::new("users")
        .where_eq("id", SqlValue::I64(1))
        .build()
        .return_sql();
    assert_eq!(sql, "SELECT * FROM users WHERE id = ?");
    assert_eq!(params, vec![SqlValue::I64(1)]);
}
