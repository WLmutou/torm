use std::collections::HashMap;
use crate::db::db_types::{SqlValue, Row, QueryResult};

/// 表结构定义
#[derive(Debug, Clone)]
pub struct TableSchema {
    pub name: String,
    pub columns: Vec<ColumnDefinition>,
    pub primary_key: Option<String>,
}

/// 列定义
#[derive(Debug, Clone)]
pub struct ColumnDefinition {
    pub name: String,
    pub column_type: ColumnType,
    pub nullable: bool,
    pub default: Option<SqlValue>,
    pub unique: bool,
}

/// 列类型
#[derive(Debug, Clone, PartialEq)]
pub enum ColumnType {
    Integer,
    BigInt,
    Real,
    Text,
    Boolean,
    Blob,
    DateTime,
}

/// 存储引擎
pub struct StorageEngine {
    tables: HashMap<String, TableData>,
}

/// 表数据
pub struct TableData {
    pub schema: TableSchema,
    pub rows: Vec<RowData>,
    pub indexes: HashMap<String, Index>,
}

/// 行数据
#[derive(Debug, Clone)]
pub struct RowData {
    pub id: u64,
    pub values: Vec<SqlValue>,
}

/// 索引
#[derive(Debug, Clone)]
pub struct Index {
    pub name: String,
    pub columns: Vec<String>,
    pub index_type: IndexType,
}

/// 索引类型
#[derive(Debug, Clone, PartialEq)]
pub enum IndexType {
    BTree,
    Hash,
}

impl StorageEngine {
    pub fn new() -> Self {
        Self {
            tables: HashMap::new(),
        }
    }

    /// 创建表
    pub fn create_table(&mut self, schema: TableSchema) -> Result<(), StorageError> {
        if self.tables.contains_key(&schema.name) {
            return Err(StorageError::TableExists(schema.name));
        }

        self.tables.insert(schema.name.clone(), TableData {
            schema,
            rows: Vec::new(),
            indexes: HashMap::new(),
        });

        Ok(())
    }

    /// 删除表
    pub fn drop_table(&mut self, table_name: &str) -> Result<(), StorageError> {
        self.tables.remove(table_name)
            .ok_or_else(|| StorageError::TableNotFound(table_name.to_string()))?;
        Ok(())
    }

    /// 插入数据
    pub fn insert(&mut self, table_name: &str, values: Vec<SqlValue>) -> Result<u64, StorageError> {
        // 先获取表 schema 用于验证（克隆以避免借用冲突）
        let schema = {
            let table = self.tables.get(table_name)
                .ok_or_else(|| StorageError::TableNotFound(table_name.to_string()))?;
            table.schema.clone()
        };

        // 验证列数
        if values.len() != schema.columns.len() {
            return Err(StorageError::ColumnCountMismatch {
                expected: schema.columns.len(),
                found: values.len(),
            });
        }

        // 验证数据类型和约束
        for (i, value) in values.iter().enumerate() {
            let column = &schema.columns[i];
            if !self.is_type_compatible(value, &column.column_type) {
                return Err(StorageError::TypeMismatch {
                    column: column.name.clone(),
                    expected: column.column_type.clone(),
                    found: self.get_value_type(value),
                });
            }

            // 检查非空约束
            if !column.nullable && matches!(value, SqlValue::Null) {
                return Err(StorageError::NotNullViolation(column.name.clone()));
            }
        }

        // 插入行数据
        let table = self.tables.get_mut(table_name)
            .ok_or_else(|| StorageError::TableNotFound(table_name.to_string()))?;
        let row_id = table.rows.len() as u64 + 1;
        table.rows.push(RowData {
            id: row_id,
            values,
        });

        Ok(row_id)
    }

    /// 查询数据
    pub fn select(&self, table_name: &str, columns: Option<Vec<String>>, 
                  where_clause: Option<WhereClause>, limit: Option<usize>, 
                  offset: Option<usize>) -> Result<QueryResult, StorageError> {
        let table = self.tables.get(table_name)
            .ok_or_else(|| StorageError::TableNotFound(table_name.to_string()))?;

        let mut matching_rows = Vec::new();

        for row_data in &table.rows {
            // 应用 WHERE 条件
            if let Some(ref clause) = where_clause {
                if !self.evaluate_where_clause(clause, row_data, &table.schema) {
                    continue;
                }
            }

            // 转换为 Row
            let row = self.row_data_to_row(row_data, &table.schema, columns.as_ref())?;
            matching_rows.push(row);
        }

        // 应用 LIMIT 和 OFFSET
        let start = offset.unwrap_or(0);
        let end = if let Some(limit) = limit {
            (start + limit).min(matching_rows.len())
        } else {
            matching_rows.len()
        };

        let paginated_rows: Vec<Row> = matching_rows.into_iter().skip(start).take(end - start).collect();

        Ok(QueryResult::new(paginated_rows))
    }

    /// 更新数据
    pub fn update(&mut self, table_name: &str, updates: HashMap<String, SqlValue>, 
                  where_clause: Option<WhereClause>) -> Result<u64, StorageError> {
        // 先克隆 schema 和行数据（避免借用冲突）
        let (schema, rows) = {
            let table = self.tables.get(table_name)
                .ok_or_else(|| StorageError::TableNotFound(table_name.to_string()))?;
            (table.schema.clone(), table.rows.clone())
        };

        // 先验证更新值的类型（避免在循环中借用 self）
        for (column_name, new_value) in &updates {
            if let Some(column_index) = schema.columns.iter().position(|c| &c.name == column_name) {
                let column = &schema.columns[column_index];
                
                if !self.is_type_compatible(new_value, &column.column_type) {
                    return Err(StorageError::TypeMismatch {
                        column: column.name.clone(),
                        expected: column.column_type.clone(),
                        found: self.get_value_type(new_value),
                    });
                }

                if !column.nullable && matches!(new_value, SqlValue::Null) {
                    return Err(StorageError::NotNullViolation(column.name.clone()));
                }
            }
        }

        // 找出所有匹配 WHERE 条件的行索引
        let mut to_update: Vec<usize> = Vec::new();
        for (idx, row_data) in rows.iter().enumerate() {
            if let Some(ref clause) = where_clause {
                if !self.evaluate_where_clause(clause, row_data, &schema) {
                    continue;
                }
            }
            to_update.push(idx);
        }

        // 应用更新
        let table = self.tables.get_mut(table_name)
            .ok_or_else(|| StorageError::TableNotFound(table_name.to_string()))?;

        let affected_count = to_update.len() as u64;

        for idx in to_update {
            if let Some(row) = table.rows.get_mut(idx) {
                for (column_name, new_value) in &updates {
                    if let Some(column_index) = schema.columns.iter().position(|c| &c.name == column_name) {
                        row.values[column_index] = new_value.clone();
                    }
                }
            }
        }

        Ok(affected_count)
    }

    /// 删除数据
    pub fn delete(&mut self, table_name: &str, where_clause: Option<WhereClause>) -> Result<u64, StorageError> {
        // 先克隆 schema 和行数据（避免借用冲突）
        let (schema, rows) = {
            let table = self.tables.get(table_name)
                .ok_or_else(|| StorageError::TableNotFound(table_name.to_string()))?;
            (table.schema.clone(), table.rows.clone())
        };

        // 找出所有匹配 WHERE 条件的行
        let to_delete: Vec<bool> = rows.iter()
            .map(|row_data| {
                if let Some(ref clause) = where_clause {
                    self.evaluate_where_clause(clause, row_data, &schema)
                } else {
                    true
                }
            })
            .collect();

        let original_count = to_delete.iter().filter(|&&b| b).count() as u64;

        // 删除匹配的行
        let table = self.tables.get_mut(table_name)
            .ok_or_else(|| StorageError::TableNotFound(table_name.to_string()))?;
        
        if where_clause.is_some() {
            let mut idx = 0;
            table.rows.retain(|_| {
                let keep = !to_delete.get(idx).copied().unwrap_or(false);
                idx += 1;
                keep
            });
        } else {
            table.rows.clear();
        }

        Ok(original_count)
    }

    /// 获取表信息
    pub fn get_table_schema(&self, table_name: &str) -> Option<&TableSchema> {
        self.tables.get(table_name).map(|table| &table.schema)
    }

    /// 获取所有表名
    pub fn get_table_names(&self) -> Vec<String> {
        self.tables.keys().cloned().collect()
    }

    // 辅助方法
    
    fn is_type_compatible(&self, value: &SqlValue, column_type: &ColumnType) -> bool {
        match (value, column_type) {
            (SqlValue::Null, _) => true, // NULL 总是兼容的（除非有 NOT NULL 约束，在其他地方检查）
            (SqlValue::I8(_) | SqlValue::I16(_) | SqlValue::I32(_), ColumnType::Integer) => true,
            (SqlValue::I64(_), ColumnType::BigInt) => true,
            (SqlValue::F32(_) | SqlValue::F64(_), ColumnType::Real) => true,
            (SqlValue::String(_), ColumnType::Text) => true,
            (SqlValue::Bool(_), ColumnType::Boolean) => true,
            (SqlValue::Bytes(_), ColumnType::Blob) => true,
            (SqlValue::DateTime(_), ColumnType::DateTime) => true,
            // 允许一些隐式转换
            (SqlValue::I8(_) | SqlValue::I16(_) | SqlValue::I32(_), ColumnType::BigInt) => true,
            (SqlValue::F32(_), ColumnType::Real) => true,
            _ => false,
        }
    }

    fn get_value_type(&self, value: &SqlValue) -> ColumnType {
        match value {
            SqlValue::Null => ColumnType::Text,
            SqlValue::Bool(_) => ColumnType::Boolean,
            SqlValue::I8(_) | SqlValue::I16(_) | SqlValue::I32(_) => ColumnType::Integer,
            SqlValue::I64(_) => ColumnType::BigInt,
            SqlValue::F32(_) | SqlValue::F64(_) => ColumnType::Real,
            SqlValue::String(_) => ColumnType::Text,
            SqlValue::Bytes(_) => ColumnType::Blob,
            SqlValue::DateTime(_) => ColumnType::DateTime,
            SqlValue::Json(_) => ColumnType::Text,
        }
    }

    fn evaluate_where_clause(&self, clause: &WhereClause, row: &RowData, schema: &TableSchema) -> bool {
        match clause {
            WhereClause::Eq(column, value) => {
                self.get_column_value(row, schema, column) == *value
            }
            WhereClause::Ne(column, value) => {
                self.get_column_value(row, schema, column) != *value
            }
            WhereClause::Gt(column, value) => {
                self.compare_values(&self.get_column_value(row, schema, column), value) > 0
            }
            WhereClause::Gte(column, value) => {
                self.compare_values(&self.get_column_value(row, schema, column), value) >= 0
            }
            WhereClause::Lt(column, value) => {
                self.compare_values(&self.get_column_value(row, schema, column), value) < 0
            }
            WhereClause::Lte(column, value) => {
                self.compare_values(&self.get_column_value(row, schema, column), value) <= 0
            }
            WhereClause::Like(column, pattern) => {
                if let SqlValue::String(col_val) = self.get_column_value(row, schema, column) {
                    if let SqlValue::String(pat) = pattern {
                        self.like_match(&col_val, pat)
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            WhereClause::And(clause1, clause2) => {
                self.evaluate_where_clause(clause1, row, schema) && 
                self.evaluate_where_clause(clause2, row, schema)
            }
            WhereClause::Or(clause1, clause2) => {
                self.evaluate_where_clause(clause1, row, schema) || 
                self.evaluate_where_clause(clause2, row, schema)
            }
            WhereClause::IsNull(column) => {
                matches!(self.get_column_value(row, schema, column), SqlValue::Null)
            }
            WhereClause::IsNotNull(column) => {
                !matches!(self.get_column_value(row, schema, column), SqlValue::Null)
            }
        }
    }

    fn get_column_value(&self, row: &RowData, schema: &TableSchema, column_name: &str) -> SqlValue {
        if let Some(index) = schema.columns.iter().position(|c| &c.name == column_name) {
            row.values.get(index).cloned().unwrap_or(SqlValue::Null)
        } else {
            SqlValue::Null
        }
    }

    fn compare_values(&self, val1: &SqlValue, val2: &SqlValue) -> i32 {
        match (val1, val2) {
            (SqlValue::I64(a), SqlValue::I64(b)) => {
                if a < b { -1 } else if a > b { 1 } else { 0 }
            }
            (SqlValue::I32(a), SqlValue::I32(b)) => {
                if a < b { -1 } else if a > b { 1 } else { 0 }
            }
            (SqlValue::F64(a), SqlValue::F64(b)) => {
                if a < b { -1 } else if a > b { 1 } else { 0 }
            }
            (SqlValue::String(a), SqlValue::String(b)) => {
                a.cmp(b) as i32
            }
            _ => 0,
        }
    }

    fn like_match(&self, text: &str, pattern: &str) -> bool {
        // 简单的 LIKE 模式匹配
        let pattern = pattern.replace("%", ".*").replace("_", ".");
        match regex_lite::Regex::new(&format!("^{}$", pattern)) {
            Ok(re) => re.is_match(text),
            Err(_) => false,
        }
    }

    fn row_data_to_row(&self, row_data: &RowData, schema: &TableSchema, 
                       columns: Option<&Vec<String>>) -> Result<Row, StorageError> {
        let (column_names, values) = if let Some(selected_columns) = columns {
            let mut names = Vec::new();
            let mut vals = Vec::new();
            
            for col_name in selected_columns {
                if let Some(index) = schema.columns.iter().position(|c| &c.name == col_name) {
                    names.push(col_name.clone());
                    vals.push(row_data.values[index].clone());
                } else {
                    return Err(StorageError::ColumnNotFound(col_name.clone()));
                }
            }
            
            (names, vals)
        } else {
            let names: Vec<String> = schema.columns.iter().map(|c| c.name.clone()).collect();
            (names, row_data.values.clone())
        };

        Ok(Row::new(column_names, values))
    }
}

impl Default for StorageEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// WHERE 子句
#[derive(Debug, Clone)]
pub enum WhereClause {
    Eq(String, SqlValue),
    Ne(String, SqlValue),
    Gt(String, SqlValue),
    Gte(String, SqlValue),
    Lt(String, SqlValue),
    Lte(String, SqlValue),
    Like(String, SqlValue),
    And(Box<WhereClause>, Box<WhereClause>),
    Or(Box<WhereClause>, Box<WhereClause>),
    IsNull(String),
    IsNotNull(String),
}

/// 存储错误
#[derive(Debug, Clone)]
pub enum StorageError {
    TableExists(String),
    TableNotFound(String),
    ColumnNotFound(String),
    ColumnCountMismatch { expected: usize, found: usize },
    TypeMismatch { column: String, expected: ColumnType, found: ColumnType },
    NotNullViolation(String),
    IndexNotFound(String),
    ConstraintViolation(String),
    IoError(String),
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageError::TableExists(name) => write!(f, "Table '{}' already exists", name),
            StorageError::TableNotFound(name) => write!(f, "Table '{}' not found", name),
            StorageError::ColumnNotFound(name) => write!(f, "Column '{}' not found", name),
            StorageError::ColumnCountMismatch { expected, found } => {
                write!(f, "Column count mismatch: expected {}, found {}", expected, found)
            }
            StorageError::TypeMismatch { column, expected, found } => {
                write!(f, "Type mismatch for column '{}': expected {:?}, found {:?}", column, expected, found)
            }
            StorageError::NotNullViolation(column) => {
                write!(f, "NOT NULL constraint violated for column '{}'", column)
            }
            StorageError::IndexNotFound(name) => write!(f, "Index '{}' not found", name),
            StorageError::ConstraintViolation(msg) => write!(f, "Constraint violation: {}", msg),
            StorageError::IoError(msg) => write!(f, "IO error: {}", msg),
        }
    }
}

impl std::error::Error for StorageError {}

/// 简单的正则表达式实现（用于 LIKE 匹配）
mod regex_lite {
    pub struct Regex {
        pattern: String,
    }

    impl Regex {
        pub fn new(pattern: &str) -> Result<Self, ()> {
            Ok(Self {
                pattern: pattern.to_string(),
            })
        }

        pub fn is_match(&self, text: &str) -> bool {
            // 非常简化的正则实现，只支持基本的通配符
            if self.pattern == ".*" {
                return true;
            }
            
            if self.pattern.contains(".*") {
                let parts: Vec<&str> = self.pattern.split(".*").collect();
                if parts.len() == 2 {
                    return text.starts_with(parts[0]) && text.ends_with(parts[1]);
                }
            }
            
            text == &self.pattern
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_table() {
        let mut engine = StorageEngine::new();
        let schema = TableSchema {
            name: "users".to_string(),
            columns: vec![
                ColumnDefinition {
                    name: "id".to_string(),
                    column_type: ColumnType::Integer,
                    nullable: false,
                    default: None,
                    unique: true,
                },
                ColumnDefinition {
                    name: "name".to_string(),
                    column_type: ColumnType::Text,
                    nullable: false,
                    default: None,
                    unique: false,
                },
            ],
            primary_key: Some("id".to_string()),
        };

        assert!(engine.create_table(schema).is_ok());
        assert!(engine.get_table_names().contains(&"users".to_string()));
    }

    #[test]
    fn test_insert_and_select() {
        let mut engine = StorageEngine::new();
        let schema = TableSchema {
            name: "users".to_string(),
            columns: vec![
                ColumnDefinition {
                    name: "id".to_string(),
                    column_type: ColumnType::Integer,
                    nullable: false,
                    default: None,
                    unique: true,
                },
                ColumnDefinition {
                    name: "name".to_string(),
                    column_type: ColumnType::Text,
                    nullable: false,
                    default: None,
                    unique: false,
                },
            ],
            primary_key: Some("id".to_string()),
        };

        engine.create_table(schema).unwrap();
        
        let values = vec![SqlValue::I32(1), SqlValue::String("Alice".to_string())];
        let row_id = engine.insert("users", values).unwrap();
        assert_eq!(row_id, 1);

        let result = engine.select("users", None, None, None, None).unwrap();
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].get("id"), Some(&SqlValue::I32(1)));
        assert_eq!(result.rows[0].get("name"), Some(&SqlValue::String("Alice".to_string())));
    }

    #[test]
    fn test_update() {
        let mut engine = StorageEngine::new();
        let schema = TableSchema {
            name: "users".to_string(),
            columns: vec![
                ColumnDefinition {
                    name: "id".to_string(),
                    column_type: ColumnType::Integer,
                    nullable: false,
                    default: None,
                    unique: true,
                },
                ColumnDefinition {
                    name: "name".to_string(),
                    column_type: ColumnType::Text,
                    nullable: false,
                    default: None,
                    unique: false,
                },
            ],
            primary_key: Some("id".to_string()),
        };

        engine.create_table(schema).unwrap();
        
        let values = vec![SqlValue::I32(1), SqlValue::String("Alice".to_string())];
        engine.insert("users", values).unwrap();

        let mut updates = HashMap::new();
        updates.insert("name".to_string(), SqlValue::String("Bob".to_string()));
        
        let affected = engine.update("users", updates, None).unwrap();
        assert_eq!(affected, 1);

        let result = engine.select("users", None, None, None, None).unwrap();
        assert_eq!(result.rows[0].get("name"), Some(&SqlValue::String("Bob".to_string())));
    }

    #[test]
    fn test_delete() {
        let mut engine = StorageEngine::new();
        let schema = TableSchema {
            name: "users".to_string(),
            columns: vec![
                ColumnDefinition {
                    name: "id".to_string(),
                    column_type: ColumnType::Integer,
                    nullable: false,
                    default: None,
                    unique: true,
                },
                ColumnDefinition {
                    name: "name".to_string(),
                    column_type: ColumnType::Text,
                    nullable: false,
                    default: None,
                    unique: false,
                },
            ],
            primary_key: Some("id".to_string()),
        };

        engine.create_table(schema).unwrap();
        
        let values = vec![SqlValue::I32(1), SqlValue::String("Alice".to_string())];
        engine.insert("users", values).unwrap();

        let affected = engine.delete("users", None).unwrap();
        assert_eq!(affected, 1);

        let result = engine.select("users", None, None, None, None).unwrap();
        assert_eq!(result.rows.len(), 0);
    }
}
