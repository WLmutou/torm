// TORM Integration Examples
// Demonstrating the new database layer functionality

use torm::db::database::Database;
use torm::orm::query::{QueryBuilder, Query};
use torm::db::db_types::SqlValue;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("TORM Integration Examples\n");
    
    // Example 1: SQLite Database Connection
    println!("=== Example 1: SQLite Connection ===");
    let db = Database::sqlite("test.db").await?;
    println!("Connected to SQLite database: {}", db.config().database);
    
    // Create a table
    let create_table_sql = r#"
        CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            email TEXT UNIQUE,
            age INTEGER,
            active BOOLEAN,
            created_at TEXT
        )
    "#;
    db.execute(create_table_sql, &[]).await?;
    println!("Created users table");
    
    // Example 2: Using QueryBuilder with SqlValue
    println!("\n=== Example 2: QueryBuilder with SqlValue ===");
    let query = QueryBuilder::new("users")
        .where_eq("name", "John")
        .where_gt("age", 18)
        .where_eq("active", true)
        .order_by("created_at", "DESC")
        .limit(10);
    
    let (sql, params) = query.build();
    println!("Generated SQL: {}", sql);
    println!("Parameters: {:?}", params);
    
    // Example 3: Advanced Query with Query struct
    println!("\n=== Example 3: Advanced Query ===");
    let query = Query::new("users")
        .where_eq("active", true)
        .where_gt("age", 18)
        .where_like("email", "%@example.com")
        .order_by_desc("created_at")
        .limit(20);
    
    let (sql, params) = query.build();
    println!("Generated SQL: {}", sql);
    println!("Parameters: {:?}", params);
    
    // Example 4: Insert operations
    println!("\n=== Example 4: Insert Operations ===");
    let insert_sql = "INSERT INTO users (name, email, age, active, created_at) VALUES (?, ?, ?, ?, ?)";
    let params = vec![
        SqlValue::String("Alice".to_string()),
        SqlValue::String("alice@example.com".to_string()),
        SqlValue::I32(25),
        SqlValue::Bool(true),
        SqlValue::String("2024-01-01 00:00:00".to_string()),
    ];
    
    match db.execute(insert_sql, &params).await {
        Ok(rows_affected) => println!("Inserted {} row(s)", rows_affected),
        Err(e) => println!("Insert error: {}", e),
    }
    
    // Example 5: Query operations
    println!("\n=== Example 5: Query Operations ===");
    let select_sql = "SELECT * FROM users WHERE age > ?";
    let params = vec![SqlValue::I32(20)];
    
    match db.query(select_sql, &params).await {
        Ok(result) => {
            println!("Found {} rows", result.rows.len());
            for row in &result.rows {
                println!("Row: {:?}", row);
            }
        }
        Err(e) => println!("Query error: {}", e),
    }
    
    // Example 6: Update operations
    println!("\n=== Example 6: Update Operations ===");
    let update_sql = "UPDATE users SET active = ? WHERE age > ?";
    let params = vec![SqlValue::Bool(false), SqlValue::I32(30)];
    
    match db.execute(update_sql, &params).await {
        Ok(rows_affected) => println!("Updated {} row(s)", rows_affected),
        Err(e) => println!("Update error: {}", e),
    }
    
    // Example 7: Delete operations
    println!("\n=== Example 7: Delete Operations ===");
    let delete_sql = "DELETE FROM users WHERE active = ?";
    let params = vec![SqlValue::Bool(false)];
    
    match db.execute(delete_sql, &params).await {
        Ok(rows_affected) => println!("Deleted {} row(s)", rows_affected),
        Err(e) => println!("Delete error: {}", e),
    }
    
    // Example 8: Transaction usage
    println!("\n=== Example 8: Transaction ===");
    match db.begin_transaction().await {
        Ok(mut transaction) => {
            // Execute multiple operations in transaction
            let insert1 = transaction.execute(
                "INSERT INTO users (name, email, age, active, created_at) VALUES (?, ?, ?, ?, ?)",
                &[
                    SqlValue::String("Bob".to_string()),
                    SqlValue::String("bob@example.com".to_string()),
                    SqlValue::I32(30),
                    SqlValue::Bool(true),
                    SqlValue::String("2024-01-02 00:00:00".to_string()),
                ]
            ).await;
            
            let insert2 = transaction.execute(
                "INSERT INTO users (name, email, age, active, created_at) VALUES (?, ?, ?, ?, ?)",
                &[
                    SqlValue::String("Charlie".to_string()),
                    SqlValue::String("charlie@example.com".to_string()),
                    SqlValue::I32(35),
                    SqlValue::Bool(true),
                    SqlValue::String("2024-01-03 00:00:00".to_string()),
                ]
            ).await;
            
            if insert1.is_ok() && insert2.is_ok() {
                match transaction.commit().await {
                    Ok(_) => println!("Transaction committed successfully"),
                    Err(e) => println!("Commit error: {}", e),
                }
            } else {
                match transaction.rollback().await {
                    Ok(_) => println!("Transaction rolled back"),
                    Err(e) => println!("Rollback error: {}", e),
                }
            }
        }
        Err(e) => println!("Transaction error: {}", e),
    }
    
    // Example 9: Complex query with multiple conditions
    println!("\n=== Example 9: Complex Query ===");
    let query = Query::new("users")
        .where_eq("active", true)
        .where_between("age", 20, 40)
        .where_in("name", vec![
            SqlValue::String("Alice".to_string()),
            SqlValue::String("Bob".to_string()),
            SqlValue::String("Charlie".to_string()),
        ])
        .order_by_asc("age")
        .limit(5);
    
    let (sql, params) = query.build();
    println!("Generated SQL: {}", sql);
    println!("Parameters: {:?}", params);
    
    // Example 10: Count and pagination
    println!("\n=== Example 10: Count and Pagination ===");
    let count_query = Query::new("users")
        .where_eq("active", true);
    
    let (count_sql, _count_params) = count_query.count();
    println!("Count SQL: {}", count_sql);
    
    let paginated_query = Query::new("users")
        .where_eq("active", true)
        .paginate(2, 10); // Page 2, 10 per page
    
    let (page_sql, _page_params) = paginated_query.build();
    println!("Pagination SQL: {}", page_sql);
    
    // Example 11: Using update with Query
    println!("\n=== Example 11: Query Update ===");
    let mut updates = HashMap::new();
    updates.insert("active".to_string(), SqlValue::Bool(false));
    updates.insert("updated_at".to_string(), SqlValue::String("2024-01-05 00:00:00".to_string()));
    
    let update_query = Query::new("users")
        .where_lt("age", 25);
    
    let (update_sql, update_params) = update_query.update(&updates);
    println!("Update SQL: {}", update_sql);
    println!("Update params: {:?}", update_params);
    
    // Example 12: Using delete with Query
    println!("\n=== Example 12: Query Delete ===");
    let delete_query = Query::new("users")
        .where_null("email");
    
    let (delete_sql, delete_params) = delete_query.delete();
    println!("Delete SQL: {}", delete_sql);
    println!("Delete params: {:?}", delete_params);
    
    // Cleanup
    db.close().await?;
    println!("\n=== Examples Complete ===");
    
    Ok(())
}