use std::fmt;

/// 数据库驱动类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbType {
    MySQL,
    PostgreSQL,
    SQLite,
}

impl fmt::Display for DbType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DbType::MySQL => write!(f, "MySQL"),
            DbType::PostgreSQL => write!(f, "PostgreSQL"),
            DbType::SQLite => write!(f, "SQLite"),
        }
    }
}

/// SQL 值类型
#[derive(Debug, Clone, PartialEq)]
pub enum SqlValue {
    Null,
    Bool(bool),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    String(String),
    Bytes(Vec<u8>),
    DateTime(chrono::DateTime<chrono::Utc>),
    Json(String),
}

impl From<bool> for SqlValue {
    fn from(v: bool) -> Self {
        SqlValue::Bool(v)
    }
}

impl From<i32> for SqlValue {
    fn from(v: i32) -> Self {
        SqlValue::I32(v)
    }
}

impl From<i64> for SqlValue {
    fn from(v: i64) -> Self {
        SqlValue::I64(v)
    }
}

impl From<String> for SqlValue {
    fn from(v: String) -> Self {
        SqlValue::String(v)
    }
}

impl From<&str> for SqlValue {
    fn from(v: &str) -> Self {
        SqlValue::String(v.to_string())
    }
}

impl SqlValue {
    pub fn to_sql_string(&self) -> String {
        match self {
            SqlValue::Null => "NULL".to_string(),
            SqlValue::Bool(v) => if *v { "TRUE" } else { "FALSE" }.to_string(),
            SqlValue::I8(v) => v.to_string(),
            SqlValue::I16(v) => v.to_string(),
            SqlValue::I32(v) => v.to_string(),
            SqlValue::I64(v) => v.to_string(),
            SqlValue::F32(v) => v.to_string(),
            SqlValue::F64(v) => v.to_string(),
            SqlValue::String(s) => format!("'{}'", s.replace('\'', "''")),
            SqlValue::Bytes(_) => "BLOB".to_string(),
            SqlValue::DateTime(dt) => format!("'{}'", dt.format("%Y-%m-%d %H:%M:%S")),
            SqlValue::Json(s) => format!("'{}'", s),
        }
    }

    pub fn as_i32(&self) -> Option<i32> {
        match self {
            SqlValue::I32(v) => Some(*v),
            SqlValue::I64(v) => Some(*v as i32),
            SqlValue::I16(v) => Some(*v as i32),
            SqlValue::I8(v) => Some(*v as i32),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            SqlValue::I64(v) => Some(*v),
            SqlValue::I32(v) => Some(*v as i64),
            SqlValue::I16(v) => Some(*v as i64),
            SqlValue::I8(v) => Some(*v as i64),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            SqlValue::String(s) => Some(s),
            SqlValue::Json(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            SqlValue::Bool(v) => Some(*v),
            SqlValue::I32(1) | SqlValue::I64(1) => Some(true),
            SqlValue::I32(0) | SqlValue::I64(0) => Some(false),
            _ => None,
        }
    }
}

/// 查询结果行
#[derive(Debug, Clone)]
pub struct Row {
    pub columns: Vec<String>,
    pub values: Vec<SqlValue>,
}

impl Row {
    pub fn new(columns: Vec<String>, values: Vec<SqlValue>) -> Self {
        Self { columns, values }
    }

    pub fn get(&self, column: &str) -> Option<&SqlValue> {
        self.columns.iter()
            .position(|c| c == column)
            .and_then(|idx| self.values.get(idx))
    }

    pub fn get_by_index(&self, index: usize) -> Option<&SqlValue> {
        self.values.get(index)
    }
}

/// 查询结果集
#[derive(Debug, Clone)]
pub struct QueryResult {
    pub rows: Vec<Row>,
    pub rows_affected: u64,
    pub last_insert_id: Option<i64>,
}

impl QueryResult {
    pub fn new(rows: Vec<Row>) -> Self {
        let rows_affected = rows.len() as u64;
        Self {
            rows,
            rows_affected,
            last_insert_id: None,
        }
    }

    pub fn empty() -> Self {
        Self {
            rows: Vec::new(),
            rows_affected: 0,
            last_insert_id: None,
        }
    }

    pub fn with_affected(rows_affected: u64) -> Self {
        Self {
            rows: Vec::new(),
            rows_affected,
            last_insert_id: None,
        }
    }

    pub fn with_insert_id(id: i64) -> Self {
        Self {
            rows: Vec::new(),
            rows_affected: 1,
            last_insert_id: Some(id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sql_value_conversions() {
        let bool_val: SqlValue = true.into();
        assert_eq!(bool_val, SqlValue::Bool(true));

        let i32_val: SqlValue = 42.into();
        assert_eq!(i32_val, SqlValue::I32(42));

        let str_val: SqlValue = "test".into();
        assert_eq!(str_val, SqlValue::String("test".to_string()));
    }

    #[test]
    fn test_sql_value_to_sql_string() {
        assert_eq!(SqlValue::Null.to_sql_string(), "NULL");
        assert_eq!(SqlValue::Bool(true).to_sql_string(), "TRUE");
        assert_eq!(SqlValue::I32(42).to_sql_string(), "42");
        assert_eq!(SqlValue::String("test".to_string()).to_sql_string(), "'test'");
    }

    #[test]
    fn test_sql_value_extraction() {
        let val: SqlValue = 42.into();
        assert_eq!(val.as_i32(), Some(42));
        assert_eq!(val.as_i64(), Some(42));
        assert_eq!(val.as_str(), None);

        let str_val: SqlValue = "hello".into();
        assert_eq!(str_val.as_str(), Some("hello"));
        assert_eq!(str_val.as_i32(), None);
    }

    #[test]
    fn test_row_operations() {
        let row = Row::new(
            vec!["id".to_string(), "name".to_string()],
            vec![SqlValue::I32(1), SqlValue::String("John".to_string())],
        );

        assert_eq!(row.get("id"), Some(&SqlValue::I32(1)));
        assert_eq!(row.get("name"), Some(&SqlValue::String("John".to_string())));
        assert_eq!(row.get("unknown"), None);
        assert_eq!(row.get_by_index(0), Some(&SqlValue::I32(1)));
    }

    #[test]
    fn test_query_result() {
        let result = QueryResult::new(vec![
            Row::new(
                vec!["id".to_string()],
                vec![SqlValue::I32(1)],
            )
        ]);

        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows_affected, 1);
    }

    #[test]
    fn test_empty_query_result() {
        let result = QueryResult::empty();
        assert!(result.rows.is_empty());
        assert_eq!(result.rows_affected, 0);
    }

    #[test]
    fn test_database_type_display() {
        assert_eq!(format!("{}", DbType::MySQL), "MySQL");
        assert_eq!(format!("{}", DbType::SQLite), "SQLite");
    }
}
