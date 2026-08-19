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
    db.create(&mut p).await.unwrap();
    assert!(p.id > 0, "auto-increment id should be set");
    assert!(p.created_at.is_some(), "created_at set by before_create hook");

    // Find back via ORM (uses derive from_row()).
    let found: Option<Person> = db.first(&p.id.to_string()).await.unwrap();
    let found = found.expect("person should be found");
    assert_eq!(found.name, "Alice");
    assert_eq!(found.age, Some(30));
    assert_eq!(found.email, "alice@example.com");
    assert_eq!(found.created_at.is_some(), true);
    // Association field defaulted.
    assert_eq!(found.tags, None);

    // Update via ORM.
    let mut found = found;
    db.update(&mut found, &[("age", 31)]).await.unwrap();
    assert!(found.updated_at.is_some(), "updated_at set by before_update hook");
    let reloaded: Option<Person> = db.first(&p.id.to_string()).await.unwrap();
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
    db.delete(&mut to_delete).await.unwrap();
    let gone: Option<Person> = db.first(&p.id.to_string()).await.unwrap();
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
    let q = Query::new("users").where_eq("name", "Alice");
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
        .where_eq("age", 31)
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
        .where_eq("age", 31)
        .query(&db)
        .select()
        .await
        .unwrap();
    assert_eq!(r.rows.len(), 1);

    // DELETE 直接执行（Query::delete(db)）
    let affected = Query::new("users")
        .where_eq("name", "Bob")
        .delete(&db)
        .await
        .unwrap();
    assert_eq!(affected, 1);

    // build() 后 return_sql() 查看 SELECT SQL
    let (sql, params) = Query::new("users")
        .where_eq("id", 1i64)
        .build()
        .return_sql();
    assert_eq!(sql, "SELECT * FROM users WHERE id = ?");
    assert_eq!(params, vec![SqlValue::I64(1)]);
}

// ------------------------------------------------------------------
// GORM-style index / schema tests: primaryKey / index / uniqueIndex.
// ------------------------------------------------------------------
#[derive(Debug, Clone, Model)]
#[model(table_name = "products", primary_key = "id")]
struct Product {
    #[model(primaryKey)]
    id: i64,
    // Named unique index; single-column uniqueIndex also implies a unique column.
    #[model(uniqueIndex = "idx_products_sku")]
    sku: String,
    // Bare `index` derives a column-named index: idx_products_category.
    #[model(index)]
    category: String,
    // Composite index: two fields share the same index name.
    #[model(index = "idx_products_name_category")]
    name: String,
    #[model(index = "idx_products_name_category")]
    category2: String,
    price: f64,
}

#[test]
fn schema_generates_columns_and_primary_key() {
    let table = Product::schema().expect("schema should be generated");
    assert_eq!(table.name, "products");

    let pk_cols: Vec<String> = table
        .columns
        .iter()
        .filter(|c| c.primary_key)
        .map(|c| c.name.clone())
        .collect();
    assert_eq!(pk_cols, vec!["id"]);

    // `sku` marked uniqueIndex is a unique column.
    let sku_col = table
        .columns
        .iter()
        .find(|c| c.name == "sku")
        .expect("sku column");
    assert!(sku_col.unique);
}

#[test]
fn schema_generates_indexes() {
    let table = Product::schema().unwrap();
    let names: Vec<&str> = table.indexes.iter().map(|i| i.name.as_str()).collect();

    assert!(names.contains(&"idx_products_sku"), "got {names:?}");
    assert!(names.contains(&"idx_products_category"), "got {names:?}");
    assert!(names.contains(&"idx_products_name_category"), "got {names:?}");

    let sku_idx = table
        .indexes
        .iter()
        .find(|i| i.name == "idx_products_sku")
        .expect("sku index");
    assert!(sku_idx.unique);
    assert_eq!(sku_idx.columns, vec!["sku"]);

    let composite = table
        .indexes
        .iter()
        .find(|i| i.name == "idx_products_name_category")
        .expect("composite index");
    assert_eq!(composite.columns, vec!["name", "category2"]);
}

#[tokio::test]
async fn auto_migrate_creates_table_and_indexes() {
    let db = torm::Database::sqlite(":memory:").await.unwrap();

    db.auto_migrate::<Product>().await.expect("auto migrate");

    // Table exists.
    let res = db
        .query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='products'",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(res.rows.len(), 1);

    // Indexes exist.
    let idx_res = db
        .query(
            "SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='products'",
            &[],
        )
        .await
        .unwrap();
    let idx_names: Vec<String> = idx_res
        .rows
        .iter()
        .map(|r| match r.get("name") {
            Some(SqlValue::String(s)) => s.clone(),
            _ => String::new(),
        })
        .collect();
    assert!(idx_names.iter().any(|n| n == "idx_products_sku"));
    assert!(idx_names.iter().any(|n| n == "idx_products_category"));
    assert!(idx_names.iter().any(|n| n == "idx_products_name_category"));

    // Idempotent.
    db.auto_migrate::<Product>().await.expect("second migrate");

    // Insert works; duplicate sku rejected by the unique index.
    let mut p1 = Product {
        id: 1,
        sku: "SKU-1".to_string(),
        category: "a".to_string(),
        name: "p1".to_string(),
        category2: "x".to_string(),
        price: 1.0,
    };
    db.create(&mut p1).await.expect("insert p1");

    let mut p2 = Product {
        id: 2,
        sku: "SKU-1".to_string(),
        category: "b".to_string(),
        name: "p2".to_string(),
        category2: "y".to_string(),
        price: 2.0,
    };
    let dup = db.create(&mut p2).await;
    assert!(
        dup.is_err(),
        "duplicate sku should be rejected by unique index"
    );
}

/// `#[derive(Model)]` 生成的 schema 应为整型主键标记自增；`auto_migrate` 建表后，
/// 以 `id = 0` 的模型 `create` 会自动分配并回填主键。
#[tokio::test]
async fn auto_migrate_auto_increment_pk_refills_id() {
    let db = torm::Database::sqlite(":memory:").await.unwrap();
    db.auto_migrate::<Product>().await.expect("auto migrate");

    // 生成的自增列在 schema 中标记为 auto_increment。
    let schema = Product::schema().unwrap();
    let pk = schema
        .columns
        .iter()
        .find(|c| c.primary_key)
        .expect("pk column");
    assert!(pk.auto_increment, "integer primary key should auto_increment");

    // id = 0 插入：主键自动分配并回填。
    let mut p1 = Product {
        id: 0,
        sku: "SKU-A1".to_string(),
        category: "a".to_string(),
        name: "p1".to_string(),
        category2: "x".to_string(),
        price: 1.0,
    };
    db.create(&mut p1).await.expect("insert p1");
    assert!(p1.id > 0, "id should be auto-assigned, got {}", p1.id);

    let mut p2 = Product {
        id: 0,
        sku: "SKU-A2".to_string(),
        category: "b".to_string(),
        name: "p2".to_string(),
        category2: "y".to_string(),
        price: 2.0,
    };
    db.create(&mut p2).await.expect("insert p2");
    assert!(p2.id > p1.id, "ids should be strictly increasing");

    // 回查验证 id 真实落库。
    let found: Option<Product> = db.first(&p1.id.to_string()).await.unwrap();
    let found = found.expect("row exists");
    assert_eq!(found.sku, "SKU-A1");
}

/// `Database::last` 应返回按主键降序的第一条（即最后插入的那条）。
#[tokio::test]
async fn last_returns_the_most_recent_model_by_pk() {
    let db = torm::Database::sqlite(":memory:").await.unwrap();
    db.auto_migrate::<Product>().await.expect("auto migrate");

    let mut products: Vec<Product> = (0..5)
        .map(|i| Product {
            id: 0,
            sku: format!("SKU-L{}", i),
            category: "c".to_string(),
            name: format!("p{}", i),
            category2: "z".to_string(),
            price: i as f64,
        })
        .collect();
    for p in &mut products {
        db.create(p).await.expect("insert");
    }

    let last: Option<Product> = db.last::<Product>().await.unwrap();
    let last = last.expect("last row exists");
    // 主键自增，最后一条应是 id 最大的那条。
    assert_eq!(last.id, products.iter().map(|p| p.id).max().unwrap());
    assert_eq!(last.name, "p4");

    // 空表时 last 返回 None。
    let empty = torm::Database::sqlite(":memory:").await.unwrap();
    empty.auto_migrate::<Product>().await.expect("migrate");
    assert!(empty.last::<Product>().await.unwrap().is_none());
}

// ------------------------------------------------------------------
// Dapper 风格：Query 结果自动映射回模型类型 + UpdateBuilder 零 SqlValue 更新。
// ------------------------------------------------------------------
#[tokio::test]
async fn query_executor_models_maps_rows_to_typed_models() {
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

    for (name, age, email) in [
        ("Alice", 25i64, "a@example.com"),
        ("Bob", 17, "b@example.com"),
        ("Carol", 30, "c@example.com"),
    ] {
        db.execute(
            "INSERT INTO people (name, age, email) VALUES (?, ?, ?)",
            &[
                SqlValue::String(name.to_string()),
                SqlValue::I64(age),
                SqlValue::String(email.to_string()),
            ],
        )
        .await
        .unwrap();
    }

    // Dapper 风格：Query::new(...).query(&db).models::<Person>() 直接得到 Vec<Person>
    let adults: Vec<Person> = Query::new("people")
        .where_gte("age", 18)
        .query(&db)
        .models::<Person>()
        .await
        .unwrap();

    let mut names: Vec<&str> = adults.iter().map(|p| p.name.as_str()).collect();
    names.sort();
    assert_eq!(names, vec!["Alice", "Carol"]);
    // 映射回类型后字段是真正的 i32 / String。
    assert!(adults.iter().all(|p| p.age.unwrap() >= 18));
}

#[tokio::test]
async fn update_with_plain_values_no_sqlvalue() {
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

    let mut p = Person {
        id: 0,
        name: "Alice".to_string(),
        age: Some(25),
        email: "old@example.com".to_string(),
        created_at: None,
        updated_at: None,
        deleted_at: None,
        tags: None,
    };
    db.create(&mut p).await.unwrap();

    // 更新：同类型列（&str）直接传原生值，零 SqlValue。
    let affected = db
        .update(
            &mut p,
            &[("name", "Alice A."), ("email", "new@example.com")],
        )
        .await
        .unwrap();
    assert_eq!(affected, 1);
    // 整数列更新同样零 SqlValue。
    db.update(&mut p, &[("age", 26)]).await.unwrap();

    let reloaded: Option<Person> = db.first(&p.id.to_string()).await.unwrap();
    let reloaded = reloaded.expect("reloaded");
    assert_eq!(reloaded.name, "Alice A.");
    assert_eq!(reloaded.age, Some(26));
    assert_eq!(reloaded.email, "new@example.com");
    assert!(reloaded.updated_at.is_some(), "updated_at refreshed by hook");
}

// ------------------------------------------------------------------
// JSON 自动同步：`#[model(json_data = "...")]` + `#[model(json = "path")]`
// 声明字段到 data JSON 的映射，`Database::update` 自动把 data 列一并写入。
// ------------------------------------------------------------------
#[derive(Debug, Clone, Model)]
#[model(table_name = "alphas", json_data = "data")]
struct AlphaRecord {
    id: i64,
    alpha_id: String,
    color: Option<String>,
    #[model(json = "color")]
    color_sync: Option<String>,
    #[model(json = "is.prodCorrelation")]
    prod_correlation: Option<f64>,
    #[model(json = "name")]
    name: String,
    data: Option<serde_json::Value>,
    created_at: Option<chrono::DateTime<Utc>>,
    updated_at: Option<chrono::DateTime<Utc>>,
    deleted_at: Option<chrono::DateTime<Utc>>,
}

#[tokio::test]
async fn derive_json_sync_updates_data_column() {
    let db = torm::Database::sqlite(":memory:").await.unwrap();
    db.execute(
        "CREATE TABLE alphas (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            alpha_id TEXT,
            color TEXT,
            color_sync TEXT,
            prod_correlation REAL,
            name TEXT,
            data TEXT,
            created_at TEXT,
            updated_at TEXT,
            deleted_at TEXT
        )",
        &[],
    )
    .await
    .unwrap();

    let mut a = AlphaRecord {
        id: 0,
        alpha_id: "A1".to_string(),
        color: None,
        color_sync: None,
        prod_correlation: None,
        name: "initial".to_string(),
        data: Some(serde_json::json!({
            "id": "A1",
            "is": { "sharpe": 1.5, "prodCorrelation": 0.3 },
            "color": "BLUE",
            "name": "initial"
        })),
        created_at: None,
        updated_at: None,
        deleted_at: None,
    };
    db.create(&mut a).await.unwrap();
    assert!(a.id > 0);

    // 更新 color 标量列 —— 期望 ORM 自动把新 color 写入 data JSON。
    a.color_sync = Some("GREEN".to_string());
    db.update(&mut a, &[("color_sync", "GREEN")]).await.unwrap();

    let reloaded: AlphaRecord = db.first(&a.id.to_string()).await.unwrap().unwrap();
    let data = reloaded.data.as_ref().expect("data should be persisted");
    // 标量列更新，且 data 中的 color 镜像同步更新。
    assert_eq!(data["color"], "GREEN");
    // 其他 data 字段保留。
    assert_eq!(data["is"]["sharpe"], 1.5);
    assert_eq!(data["name"], "initial");

    // 更新嵌套字段 prod_correlation -> data.is.prodCorrelation。
    let mut reloaded = reloaded;
    reloaded.prod_correlation = Some(0.9);
    db.update(&mut reloaded, &[("prod_correlation", 0.9)])
        .await
        .unwrap();
    let reloaded2: AlphaRecord = db.first(&a.id.to_string()).await.unwrap().unwrap();
    assert_eq!(reloaded2.data.unwrap()["is"]["prodCorrelation"], 0.9);
}

#[tokio::test]
async fn derive_json_sync_skips_unrelated_updates() {
    let db = torm::Database::sqlite(":memory:").await.unwrap();
    db.execute(
        "CREATE TABLE alphas (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            alpha_id TEXT,
            color TEXT,
            color_sync TEXT,
            prod_correlation REAL,
            name TEXT,
            data TEXT,
            created_at TEXT,
            updated_at TEXT,
            deleted_at TEXT
        )",
        &[],
    )
    .await
    .unwrap();

    let mut a = AlphaRecord {
        id: 0,
        alpha_id: "A2".to_string(),
        color: None,
        color_sync: None,
        prod_correlation: None,
        name: "n1".to_string(),
        data: Some(serde_json::json!({ "name": "n1", "color": "BLUE" })),
        created_at: None,
        updated_at: None,
        deleted_at: None,
    };
    db.create(&mut a).await.unwrap();

    // 仅更新未声明 json 映射的 alpha_id —— data 不应被改写。
    let mut a = a;
    a.alpha_id = "A2b".to_string();
    db.update(&mut a, &[("alpha_id", "A2b")]).await.unwrap();

    let reloaded: AlphaRecord = db.first(&a.id.to_string()).await.unwrap().unwrap();
    let data = reloaded.data.unwrap();
    assert_eq!(data["color"], "BLUE", "unrelated update must not touch data");
    assert_eq!(data["name"], "n1");
    assert_eq!(reloaded.alpha_id, "A2b");
}
