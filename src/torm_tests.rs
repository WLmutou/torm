#[cfg(test)]
mod tests {
    use crate::db::error::TormError;
    use crate::db::db_types::SqlValue;
    use crate::orm::model::{Model, Timestamps};
    use crate::orm::query::{Query, QueryBuilder, OrderDirection};
    use crate::db::driver::{Dsn, DBDriver};

    // Test model
    #[derive(Debug, Clone)]
    pub struct User {
        pub id: String,
        pub name: String,
        pub email: String,
        pub age: Option<i32>,
        pub status: String,
        pub timestamps: Timestamps,
    }

    impl User {
        pub fn new(name: &str, email: &str) -> Self {
            Self {
                id: String::new(),
                name: name.to_string(),
                email: email.to_string(),
                age: None,
                status: "active".to_string(),
                timestamps: Timestamps::new(),
            }
        }

        pub fn with_age(mut self, age: i32) -> Self {
            self.age = Some(age);
            self
        }

        pub fn with_status(mut self, status: &str) -> Self {
            self.status = status.to_string();
            self
        }
    }

    #[async_trait::async_trait]
    impl crate::orm::model::Model for User {
        fn table_name() -> &'static str {
            "users"
        }

        fn id(&self) -> Option<String> {
            if self.id.is_empty() {
                None
            } else {
                Some(self.id.clone())
            }
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

        fn set_created_at(&mut self, timestamp: chrono::DateTime<chrono::Utc>) {
            self.timestamps.created_at = Some(timestamp);
        }

        fn set_updated_at(&mut self, timestamp: chrono::DateTime<chrono::Utc>) {
            self.timestamps.updated_at = Some(timestamp);
        }

        fn set_deleted_at(&mut self, timestamp: Option<chrono::DateTime<chrono::Utc>>) {
            self.timestamps.deleted_at = timestamp;
        }
    }

    #[test]
    fn test_user_creation() {
        let user = User::new("John Doe", "john@example.com");
        assert_eq!(user.name, "John Doe");
        assert_eq!(user.email, "john@example.com");
        assert_eq!(user.status, "active");
        assert!(user.age.is_none());
        assert!(user.id.is_empty());
    }

    #[test]
    fn test_user_builder_pattern() {
        let user = User::new("Jane Doe", "jane@example.com")
            .with_age(30)
            .with_status("verified");
        
        assert_eq!(user.name, "Jane Doe");
        assert_eq!(user.age, Some(30));
        assert_eq!(user.status, "verified");
    }

    #[test]
    fn test_timestamps_initialization() {
        let user = User::new("Test User", "test@example.com");
        assert!(user.created_at().is_none());
        assert!(user.updated_at().is_none());
        assert!(user.deleted_at().is_none());
        assert!(!user.is_deleted());
    }

    #[test]
    fn test_soft_delete() {
        let mut user = User::new("Test User", "test@example.com");
        assert!(!user.is_deleted());
        
        let now = chrono::Utc::now();
        user.set_deleted_at(Some(now));
        assert!(user.is_deleted());
        assert_eq!(user.deleted_at(), Some(now));
    }

    #[test]
    fn test_query_builder_basic() {
        let builder = QueryBuilder::new("users")
            .where_eq("name", "John")
            .limit(10);
        
        let (sql, bindings) = builder.build();
        assert_eq!(sql, "SELECT * FROM users WHERE name = ? LIMIT 10");
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0], SqlValue::String("John".to_string()));
    }

    #[test]
    fn test_query_builder_multiple_conditions() {
        let builder = QueryBuilder::new("users")
            .where_eq("status", "active")
            .where_gt("age", 18)
            .order_by("created_at", "DESC");
        
        let (sql, bindings) = builder.build();
        assert_eq!(sql, "SELECT * FROM users WHERE status = ? AND age > ? ORDER BY created_at DESC");
        assert_eq!(bindings.len(), 2);
    }

    #[test]
    fn test_query_in_condition() {
        let builder = QueryBuilder::new("users")
            .where_in("id", vec![SqlValue::I32(1), SqlValue::I32(2), SqlValue::I32(3)]);
        
        let (sql, bindings) = builder.build();
        assert_eq!(sql, "SELECT * FROM users WHERE id IN (?, ?, ?)");
        assert_eq!(bindings.len(), 3);
    }

    #[test]
    fn test_query_between_condition() {
        let builder = QueryBuilder::new("users")
            .where_between("age", 18, 65);
        
        let (sql, bindings) = builder.build();
        assert_eq!(sql, "SELECT * FROM users WHERE age BETWEEN ? AND ?");
        assert_eq!(bindings.len(), 2);
    }

    #[test]
    fn test_query_null_conditions() {
        let builder_null = QueryBuilder::new("users")
            .where_null("deleted_at");
        let (sql_null, _) = builder_null.build();
        assert_eq!(sql_null, "SELECT * FROM users WHERE deleted_at IS NULL");
        
        let builder_not_null = QueryBuilder::new("users")
            .where_not_null("email");
        let (sql_not_null, _) = builder_not_null.build();
        assert_eq!(sql_not_null, "SELECT * FROM users WHERE email IS NOT NULL");
    }

    #[test]
    fn test_query_fluent_api() {
        let query = Query::new("users")
            .where_eq("status", "active")
            .where_gt("age", 18)
            .order_by_desc("created_at")
            .limit(20);
        
        let (sql, bindings) = query.build().return_sql();
        assert_eq!(sql, "SELECT * FROM users WHERE status = ? AND age > ? ORDER BY created_at DESC LIMIT 20");
        assert_eq!(bindings.len(), 2);
    }

    #[test]
    fn test_query_pagination() {
        let query = Query::new("users").paginate(2, 10);
        let (sql, bindings) = query.build().return_sql();
        assert_eq!(sql, "SELECT * FROM users LIMIT 10 OFFSET 10");
        assert_eq!(bindings, Vec::<SqlValue>::new());
    }

    #[test]
    fn test_query_count() {
        let query = Query::new("users")
            .where_eq("status", "active")
            .where_null("deleted_at");
        
        let (sql, bindings) = query.count().return_sql();
        assert_eq!(sql, "SELECT COUNT(*) FROM users WHERE status = ? AND deleted_at IS NULL");
        assert_eq!(bindings.len(), 1);
    }

    #[test]
    fn test_query_update() {
        let query = Query::new("users").where_eq("id", "123");
        let mut updates = std::collections::HashMap::new();
        updates.insert("name".to_string(), SqlValue::String("John Doe".to_string()));
        updates.insert("age".to_string(), SqlValue::I32(25));
        
        let (sql, bindings) = query.build_update(&updates).return_sql();
        // HashMap 迭代顺序不确定
        assert!(sql.starts_with("UPDATE users SET"));
        assert!(sql.contains("name = ?"));
        assert!(sql.contains("age = ?"));
        assert!(sql.ends_with("WHERE id = ?"));
        assert_eq!(bindings.len(), 3);
    }

    #[test]
    fn test_query_delete() {
        let query = Query::new("users").where_eq("id", "123");
        let (sql, bindings) = query.build_delete().return_sql();
        assert_eq!(sql, "DELETE FROM users WHERE id = ?");
        assert_eq!(bindings.len(), 1);
    }

    #[test]
    fn test_dsn_default() {
        let dsn = Dsn::default();
        assert_eq!(dsn.driver, DBDriver::SQLite);
        assert_eq!(dsn.database, "torm.db");
    }

    #[test]
    fn test_dsn_builder() {
        let dsn = Dsn::new(DBDriver::MySQL, "mydb")
            .with_host("localhost")
            .with_port(3306)
            .with_username("user")
            .with_password("pass");
        
        assert_eq!(dsn.host, "localhost");
        assert_eq!(dsn.port, 3306);
        assert_eq!(dsn.username, "user");
        assert_eq!(dsn.password, "pass");
        assert!(dsn.build().contains("localhost:3306"));
    }

    #[test]
    fn test_dsn_build_sqlite() {
        let dsn = Dsn::new(DBDriver::SQLite, "test.db");
        let connection_string = dsn.build();
        assert_eq!(connection_string, "test.db");
    }

    #[test]
    fn test_dsn_build_mysql() {
        let dsn = Dsn::new(DBDriver::MySQL, "mydb")
            .with_host("localhost")
            .with_port(3306)
            .with_username("root")
            .with_password("password");
        
        let connection_string = dsn.build();
        assert!(connection_string.contains("mysql://"));
        assert!(connection_string.contains("root:password@localhost:3306/mydb"));
    }

    #[test]
    fn test_dsn_build_postgresql() {
        let dsn = Dsn::new(DBDriver::PostgreSQL, "mydb")
            .with_host("localhost")
            .with_port(5432)
            .with_username("postgres")
            .with_password("password");
        
        let connection_string = dsn.build();
        assert!(connection_string.contains("postgresql://"));
        assert!(connection_string.contains("postgres:password@localhost:5432/mydb"));
    }

    #[test]
    fn test_error_creation() {
        let error = TormError::custom("Custom error message");
        assert!(error.to_string().contains("Custom error message"));
        
        let invalid_error = TormError::invalid_query("Invalid WHERE clause");
        assert!(invalid_error.to_string().contains("Invalid query"));
    }

    #[test]
    fn test_order_direction() {
        let asc_str = OrderDirection::Asc.as_str();
        let desc_str = OrderDirection::Desc.as_str();
        
        assert_eq!(asc_str, "ASC");
        assert_eq!(desc_str, "DESC");
    }

    #[test]
    fn test_query_default() {
        let query = Query::default();
        let (sql, _) = query.build().return_sql();
        assert_eq!(sql, "SELECT * FROM ");
    }

    #[test]
    fn test_complex_query_with_like() {
        let query = Query::new("users")
            .where_like("name", "John%")
            .where_like("email", "%@example.com");
        
        let (sql, bindings) = query.build().return_sql();
        assert_eq!(sql, "SELECT * FROM users WHERE name LIKE ? AND email LIKE ?");
        assert_eq!(bindings.len(), 2);
    }

    #[test]
    fn test_query_with_limit_offset() {
        let query = Query::new("users")
            .limit(20)
            .offset(40);
        
        let (sql, bindings) = query.build().return_sql();
        assert_eq!(sql, "SELECT * FROM users LIMIT 20 OFFSET 40");
        assert_eq!(bindings, Vec::<SqlValue>::new());
    }

    #[test]
    fn test_query_with_not_in() {
        let query = Query::new("users")
            .where_not_in("status", vec![
                SqlValue::String("deleted".to_string()),
                SqlValue::String("banned".to_string()),
            ]);
        
        let (sql, bindings) = query.build().return_sql();
        assert_eq!(sql, "SELECT * FROM users WHERE status NOT IN (?, ?)");
        assert_eq!(bindings.len(), 2);
    }

    #[test]
    fn test_query_with_multiple_order() {
        let query = Query::new("users")
            .order_by_asc("name")
            .order_by_desc("created_at");
        
        let (sql, bindings) = query.build().return_sql();
        assert_eq!(sql, "SELECT * FROM users ORDER BY name ASC, created_at DESC");
        assert_eq!(bindings, Vec::<SqlValue>::new());
    }

    #[test]
    fn test_query_with_comparison_operators() {
        // Test all comparison operators
        let eq_query = Query::new("users").where_eq("age", 25);
        let (eq_sql, eq_bindings) = eq_query.build().return_sql();
        assert_eq!(eq_sql, "SELECT * FROM users WHERE age = ?");
        assert_eq!(eq_bindings.len(), 1);

        let ne_query = Query::new("users").where_ne("status", "active");
        let (ne_sql, ne_bindings) = ne_query.build().return_sql();
        assert_eq!(ne_sql, "SELECT * FROM users WHERE status != ?");
        assert_eq!(ne_bindings.len(), 1);

        let gt_query = Query::new("users").where_gt("age", 18);
        let (gt_sql, gt_bindings) = gt_query.build().return_sql();
        assert_eq!(gt_sql, "SELECT * FROM users WHERE age > ?");
        assert_eq!(gt_bindings.len(), 1);

        let gte_query = Query::new("users").where_gte("age", 18);
        let (gte_sql, gte_bindings) = gte_query.build().return_sql();
        assert_eq!(gte_sql, "SELECT * FROM users WHERE age >= ?");
        assert_eq!(gte_bindings.len(), 1);

        let lt_query = Query::new("users").where_lt("age", 65);
        let (lt_sql, lt_bindings) = lt_query.build().return_sql();
        assert_eq!(lt_sql, "SELECT * FROM users WHERE age < ?");
        assert_eq!(lt_bindings.len(), 1);

        let lte_query = Query::new("users").where_lte("age", 65);
        let (lte_sql, lte_bindings) = lte_query.build().return_sql();
        assert_eq!(lte_sql, "SELECT * FROM users WHERE age <= ?");
        assert_eq!(lte_bindings.len(), 1);
    }

    #[test]
    fn test_query_builder_order_by_with_direction() {
        let builder = QueryBuilder::new("users")
            .order_by("name", "ASC")
            .order_by("created_at", "desc")  // Test case insensitive
            .limit(10);
        
        let (sql, _) = builder.build();
        assert_eq!(sql, "SELECT * FROM users ORDER BY name ASC, created_at DESC LIMIT 10");
    }

    #[test]
    fn test_query_update_multiple_fields() {
        let query = Query::new("users").where_eq("id", "123");
        let mut updates = std::collections::HashMap::new();
        updates.insert("name".to_string(), SqlValue::String("John Doe".to_string()));
        updates.insert("email".to_string(), SqlValue::String("john@example.com".to_string()));
        updates.insert("age".to_string(), SqlValue::I32(25));
        updates.insert("status".to_string(), SqlValue::String("active".to_string()));
        
        let (sql, bindings) = query.build_update(&updates).return_sql();
        assert!(sql.contains("UPDATE users SET"));
        assert!(sql.contains("WHERE id = ?"));
        assert_eq!(bindings.len(), 5); // 4 update fields + 1 where condition
    }

    #[test]
    fn test_query_delete_without_where() {
        let query = Query::new("users");
        let (sql, bindings) = query.build_delete().return_sql();
        assert_eq!(sql, "DELETE FROM users");
        assert_eq!(bindings, Vec::<SqlValue>::new());
    }

    #[test]
    fn test_query_count_without_where() {
        let query = Query::new("users");
        let (sql, bindings) = query.count().return_sql();
        assert_eq!(sql, "SELECT COUNT(*) FROM users");
        assert_eq!(bindings, Vec::<SqlValue>::new());
    }

    #[test]
    fn test_query_builder_limit_offset() {
        let builder = QueryBuilder::new("users")
            .where_eq("status", "active")
            .limit(10)
            .offset(20);
        
        let (sql, bindings) = builder.build();
        assert_eq!(sql, "SELECT * FROM users WHERE status = ? LIMIT 10 OFFSET 20");
        assert_eq!(bindings.len(), 1);
    }
}
