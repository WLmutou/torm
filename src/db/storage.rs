use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use crate::db::db_types::{SqlValue, Row, QueryResult};

/// 表结构定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSchema {
    pub name: String,
    pub columns: Vec<ColumnDefinition>,
    pub primary_key: Option<String>,
}

/// 列定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDefinition {
    pub name: String,
    pub column_type: ColumnType,
    pub nullable: bool,
    pub default: Option<SqlValue>,
    pub unique: bool,
}

/// 列类型
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableData {
    pub schema: TableSchema,
    pub rows: Vec<RowData>,
    pub indexes: HashMap<String, Index>,
}

/// 行数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RowData {
    pub id: u64,
    pub values: Vec<SqlValue>,
}

/// 索引
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Index {
    pub name: String,
    pub columns: Vec<String>,
    pub index_type: IndexType,
}

/// 索引类型
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

    pub fn save_to_file(&self, path: &str) -> Result<(), StorageError> {
        let bytes = self.encode()?;
        std::fs::write(path, bytes)
            .map_err(|e| StorageError::IoError(format!("Failed to write file: {}", e)))?;
        Ok(())
    }

    pub fn load_from_file(path: &str) -> Result<Self, StorageError> {
        let bytes = std::fs::read(path)
            .map_err(|e| StorageError::IoError(format!("Failed to read file: {}", e)))?;
        Self::decode(&bytes)
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
            let rows = std::mem::take(&mut table.rows);
            table.rows = rows
                .into_iter()
                .enumerate()
                .filter(|(idx, _)| !to_delete.get(*idx).copied().unwrap_or(false))
                .map(|(_, row)| row)
                .collect();
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
        let text: Vec<char> = text.chars().collect();
        let pattern: Vec<char> = pattern.chars().collect();
        Self::like_matcher(&text, &pattern)
    }

    fn like_matcher(text: &[char], pattern: &[char]) -> bool {
        let mut ti = 0;
        let mut pi = 0;
        let mut star_idx: Option<usize> = None;
        let mut match_idx: usize = 0;

        while ti < text.len() {
            if pi < pattern.len() && (pattern[pi] == '_' || pattern[pi] == text[ti]) {
                ti += 1;
                pi += 1;
            } else if pi < pattern.len() && pattern[pi] == '%' {
                star_idx = Some(pi);
                pi += 1;
            } else if let Some(si) = star_idx {
                pi = si + 1;
                match_idx += 1;
                ti = match_idx;
            } else {
                return false;
            }
        }

        while pi < pattern.len() && pattern[pi] == '%' {
            pi += 1;
        }

        pi == pattern.len()
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

// ============================================================
// 二进制持久化
// 自定义二进制格式（无外部依赖），文件头 + 表数据：
//   Magic: "TORMDB01" (8 bytes)
//   Version: u32
//   Table count: u32
//   每个表: 表名 / 列数 / 列定义 / 主键 / 行数 / 行数据
// ============================================================
const DB_MAGIC: &[u8; 8] = b"TORMDB01";
const DB_VERSION: u32 = 1;

impl StorageEngine {
    /// 将整个存储引擎编码为二进制字节
    pub fn encode(&self) -> Result<Vec<u8>, StorageError> {
        let mut buf: Vec<u8> = Vec::new();
        buf.extend_from_slice(DB_MAGIC);
        put_u32(&mut buf, DB_VERSION);
        put_u32(&mut buf, self.tables.len() as u32);

        // 按表名排序以确保确定性输出
        let mut table_names: Vec<&String> = self.tables.keys().collect();
        table_names.sort();

        for name in table_names {
            let table = &self.tables[name];
            // 表名
            put_str(&mut buf, &table.schema.name);
            // 列数
            put_u32(&mut buf, table.schema.columns.len() as u32);
            // 列定义
            for col in &table.schema.columns {
                put_str(&mut buf, &col.name);
                put_u8(&mut buf, column_type_to_u8(&col.column_type));
                put_u8(&mut buf, col.nullable as u8);
                put_u8(&mut buf, col.unique as u8);
                // 默认值
                match &col.default {
                    Some(v) => {
                        put_u8(&mut buf, 1);
                        put_sql_value(&mut buf, v);
                    }
                    None => put_u8(&mut buf, 0),
                }
            }
            // 主键
            match &table.schema.primary_key {
                Some(pk) => {
                    put_u8(&mut buf, 1);
                    put_str(&mut buf, pk);
                }
                None => put_u8(&mut buf, 0),
            }
            // 行数
            put_u64(&mut buf, table.rows.len() as u64);
            // 行数据
            for row in &table.rows {
                put_u64(&mut buf, row.id);
                put_u32(&mut buf, row.values.len() as u32);
                for v in &row.values {
                    put_sql_value(&mut buf, v);
                }
            }
        }

        Ok(buf)
    }

    /// 从二进制字节解码存储引擎
    pub fn decode(bytes: &[u8]) -> Result<Self, StorageError> {
        let mut reader = Reader::new(bytes);

        // 校验魔数
        let magic = reader.read_bytes(8)?;
        if magic != DB_MAGIC.as_slice() {
            return Err(StorageError::IoError(
                "Invalid database file: bad magic".to_string(),
            ));
        }

        let _version = reader.u32()?;
        let table_count = reader.u32()? as usize;

        let mut tables: HashMap<String, TableData> = HashMap::new();
        for _ in 0..table_count {
            let name = reader.str()?;
            let column_count = reader.u32()? as usize;

            let mut columns = Vec::with_capacity(column_count);
            for _ in 0..column_count {
                let col_name = reader.str()?;
                let ct = column_type_from_u8(reader.u8()?)?;
                let nullable = reader.u8()? != 0;
                let unique = reader.u8()? != 0;
                let has_default = reader.u8()? != 0;
                let default = if has_default {
                    Some(reader.sql_value()?)
                } else {
                    None
                };
                columns.push(ColumnDefinition {
                    name: col_name,
                    column_type: ct,
                    nullable,
                    default,
                    unique,
                });
            }

            let has_pk = reader.u8()? != 0;
            let primary_key = if has_pk {
                Some(reader.str()?)
            } else {
                None
            };

            let row_count = reader.u64()? as usize;
            let mut rows = Vec::with_capacity(row_count);
            for _ in 0..row_count {
                let id = reader.u64()?;
                let value_count = reader.u32()? as usize;
                let mut values = Vec::with_capacity(value_count);
                for _ in 0..value_count {
                    values.push(reader.sql_value()?);
                }
                rows.push(RowData { id, values });
            }

            tables.insert(
                name.clone(),
                TableData {
                    schema: TableSchema {
                        name,
                        columns,
                        primary_key,
                    },
                    rows,
                    indexes: HashMap::new(),
                },
            );
        }

        Ok(Self { tables })
    }
}

// ---- 编码辅助函数 ----

fn put_u8(buf: &mut Vec<u8>, v: u8) {
    buf.push(v);
}

fn put_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn put_u64(buf: &mut Vec<u8>, v: u64) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn put_str(buf: &mut Vec<u8>, s: &str) {
    put_u32(buf, s.len() as u32);
    buf.extend_from_slice(s.as_bytes());
}

fn put_sql_value(buf: &mut Vec<u8>, v: &SqlValue) {
    match v {
        SqlValue::Null => buf.push(0),
        SqlValue::Bool(b) => {
            buf.push(1);
            buf.push(*b as u8);
        }
        SqlValue::I8(i) => {
            buf.push(2);
            buf.extend_from_slice(&(*i as i32).to_le_bytes());
        }
        SqlValue::I16(i) => {
            buf.push(2);
            buf.extend_from_slice(&(*i as i32).to_le_bytes());
        }
        SqlValue::I32(i) => {
            buf.push(2);
            buf.extend_from_slice(&i.to_le_bytes());
        }
        SqlValue::I64(i) => {
            buf.push(3);
            buf.extend_from_slice(&i.to_le_bytes());
        }
        SqlValue::F32(f) => {
            buf.push(4);
            buf.extend_from_slice(&(*f as f64).to_le_bytes());
        }
        SqlValue::F64(f) => {
            buf.push(4);
            buf.extend_from_slice(&f.to_le_bytes());
        }
        SqlValue::String(s) => {
            buf.push(5);
            put_str(buf, s);
        }
        SqlValue::Bytes(b) => {
            buf.push(6);
            put_u32(buf, b.len() as u32);
            buf.extend_from_slice(b);
        }
        SqlValue::DateTime(dt) => {
            buf.push(7);
            buf.extend_from_slice(&dt.timestamp_nanos_opt().unwrap_or(0).to_le_bytes());
        }
        SqlValue::Json(s) => {
            buf.push(8);
            put_str(buf, s);
        }
    }
}

fn column_type_to_u8(ct: &ColumnType) -> u8 {
    match ct {
        ColumnType::Integer => 0,
        ColumnType::BigInt => 1,
        ColumnType::Real => 2,
        ColumnType::Text => 3,
        ColumnType::Boolean => 4,
        ColumnType::Blob => 5,
        ColumnType::DateTime => 6,
    }
}

fn column_type_from_u8(v: u8) -> Result<ColumnType, StorageError> {
    Ok(match v {
        0 => ColumnType::Integer,
        1 => ColumnType::BigInt,
        2 => ColumnType::Real,
        3 => ColumnType::Text,
        4 => ColumnType::Boolean,
        5 => ColumnType::Blob,
        6 => ColumnType::DateTime,
        _ => {
            return Err(StorageError::IoError(format!(
                "Invalid column type tag: {}",
                v
            )))
        }
    })
}

// ---- 读取辅助 ----

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn read_bytes(&mut self, len: usize) -> Result<&'a [u8], StorageError> {
        if self.pos + len > self.buf.len() {
            return Err(StorageError::IoError(
                "Unexpected end of database file".to_string(),
            ));
        }
        let slice = &self.buf[self.pos..self.pos + len];
        self.pos += len;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, StorageError> {
        Ok(self.read_bytes(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, StorageError> {
        let b = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&mut self) -> Result<u64, StorageError> {
        let b = self.read_bytes(8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    fn str(&mut self) -> Result<String, StorageError> {
        let len = self.u32()? as usize;
        let bytes = self.read_bytes(len)?;
        String::from_utf8(bytes.to_vec())
            .map_err(|_| StorageError::IoError("Invalid UTF-8 in database file".to_string()))
    }

    fn sql_value(&mut self) -> Result<SqlValue, StorageError> {
        let tag = self.u8()?;
        Ok(match tag {
            0 => SqlValue::Null,
            1 => SqlValue::Bool(self.u8()? != 0),
            2 => {
                let b = self.read_bytes(4)?;
                SqlValue::I32(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            }
            3 => {
                let b = self.read_bytes(8)?;
                SqlValue::I64(i64::from_le_bytes([
                    b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
                ]))
            }
            4 => {
                let b = self.read_bytes(8)?;
                SqlValue::F64(f64::from_le_bytes([
                    b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
                ]))
            }
            5 => SqlValue::String(self.str()?),
            6 => {
                let len = self.u32()? as usize;
                SqlValue::Bytes(self.read_bytes(len)?.to_vec())
            }
            7 => {
                let b = self.read_bytes(8)?;
                let nanos = i64::from_le_bytes([
                    b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
                ]);
                SqlValue::DateTime(chrono::DateTime::from_timestamp_nanos(nanos))
            }
            8 => SqlValue::Json(self.str()?),
            _ => {
                return Err(StorageError::IoError(format!(
                    "Invalid value tag: {}",
                    tag
                )))
            }
        })
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
