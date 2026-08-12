use crate::db::database::{Database, DbError};
use crate::db::db_types::{DbType, QueryResult, SqlValue};
use crate::orm::model::Model;
use crate::utils::sql_safety::validate_qualified_identifier;
use std::collections::HashMap;

/// 校验并规范化 WHERE/ORDER 等子句中的列标识符。
///
/// 允许限定名（`users.id`）与聚合写法（`COUNT(*)`），
/// 拒绝含单引号、分号、注释等危险字符的输入；非法时返回空串并告警。
fn safe_column(col: &str) -> String {
    validate_qualified_identifier(col).unwrap_or_else(|e| {
        eprintln!("[torm::sql_safety] rejected unsafe column: {e}");
        String::new()
    })
}

/// 校验并规范化表名标识符。
///
/// 表名可能携带别名（如 `roles r`、`roles AS r`），因此使用宽松校验：
/// 允许字母/数字/`_`/`$`/`.`/空格（用于别名）与聚合括号，但仍拒绝
/// `;`、单引号、注释及危险关键字等注入特征。
fn safe_identifier(id: &str) -> String {
    validate_qualified_identifier(id).unwrap_or_else(|e| {
        eprintln!("[torm::sql_safety] rejected unsafe identifier: {e}");
        String::new()
    })
}

/// 一条已构建好、可立即执行的 SQL 语句及其绑定参数。
///
/// 由 [`Query::build`]、[`Query::count`]、[`Query::insert`]、
/// [`Query::update`]、[`Query::delete`] 等返回。
///
/// 提供两种使用方式：
/// - [`SqlStatement::execute`] / [`SqlStatement::query`]：直接对数据库执行。
/// - [`SqlStatement::return_sql`]：仅查看生成的 SQL 与参数（`(sql, params)`）。
#[derive(Clone)]
pub struct SqlStatement {
    sql: String,
    params: Vec<SqlValue>,
}

impl SqlStatement {
    pub(crate) fn new(sql: String, params: Vec<SqlValue>) -> Self {
        Self { sql, params }
    }

    /// 返回生成的 SQL 文本。
    pub fn sql(&self) -> &str {
        &self.sql
    }

    /// 返回绑定的参数。
    pub fn params(&self) -> &[SqlValue] {
        &self.params
    }

    /// 查看 SQL 与绑定参数，返回 `(sql, params)`。
    pub fn return_sql(&self) -> (String, Vec<SqlValue>) {
        (self.sql.clone(), self.params.clone())
    }

    /// 直接执行写语句（INSERT / UPDATE / DELETE / DDL），返回受影响行数。
    ///
    /// 会根据数据库方言自动转换占位符（PostgreSQL 使用 `$n`，SQLite / MySQL 使用 `?`）。
    pub async fn execute(&self, db: &Database) -> Result<u64, DbError> {
        let sql = match db.db_type() {
            DbType::PostgreSQL => convert_placeholders(&self.sql),
            _ => self.sql.clone(),
        };
        db.execute(&sql, &self.params).await
    }

    /// 直接执行查询（SELECT / COUNT），返回查询结果集。
    pub async fn query(&self, db: &Database) -> Result<QueryResult, DbError> {
        let sql = match db.db_type() {
            DbType::PostgreSQL => convert_placeholders(&self.sql),
            _ => self.sql.clone(),
        };
        db.query(&sql, &self.params).await
    }
}

/// 将 SQL 中的 `?` 占位符转换为 PostgreSQL 的 `$1/$2/...`。
/// 跳过单引号字符串字面量（含 `''` 转义）与 `--` 行注释。
pub(crate) fn convert_placeholders(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let mut in_single = false;
    let mut n: usize = 0;
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_single {
            out.push(c);
            if c == '\'' {
                if i + 1 < chars.len() && chars[i + 1] == '\'' {
                    out.push('\'');
                    i += 1;
                } else {
                    in_single = false;
                }
            }
        } else if c == '\'' {
            in_single = true;
            out.push(c);
        } else if c == '-' && i + 1 < chars.len() && chars[i + 1] == '-' {
            while i < chars.len() && chars[i] != '\n' {
                out.push(chars[i]);
                i += 1;
            }
            continue;
        } else if c == '?' {
            n += 1;
            out.push('$');
            out.push_str(&n.to_string());
        } else {
            out.push(c);
        }
        i += 1;
    }
    out
}

pub struct QueryBuilder {
    table_name: String,
    conditions: Vec<String>,
    orders: Vec<String>,
    limit: Option<u64>,
    offset: Option<u64>,
    bindings: Vec<SqlValue>,
}

impl QueryBuilder {
    pub fn new(table_name: &str) -> Self {
        Self {
            table_name: safe_identifier(table_name),
            conditions: Vec::new(),
            orders: Vec::new(),
            limit: None,
            offset: None,
            bindings: Vec::new(),
        }
    }

    pub fn where_eq(mut self, column: &str, value: impl Into<SqlValue>) -> Self {
        self.conditions.push(format!("{} = ?", safe_column(column)));
        self.bindings.push(value.into());
        self
    }

    pub fn where_ne(mut self, column: &str, value: impl Into<SqlValue>) -> Self {
        self.conditions.push(format!("{} != ?", safe_column(column)));
        self.bindings.push(value.into());
        self
    }

    pub fn where_gt(mut self, column: &str, value: impl Into<SqlValue>) -> Self {
        self.conditions.push(format!("{} > ?", safe_column(column)));
        self.bindings.push(value.into());
        self
    }

    pub fn where_gte(mut self, column: &str, value: impl Into<SqlValue>) -> Self {
        self.conditions.push(format!("{} >= ?", safe_column(column)));
        self.bindings.push(value.into());
        self
    }

    pub fn where_lt(mut self, column: &str, value: impl Into<SqlValue>) -> Self {
        self.conditions.push(format!("{} < ?", safe_column(column)));
        self.bindings.push(value.into());
        self
    }

    pub fn where_lte(mut self, column: &str, value: impl Into<SqlValue>) -> Self {
        self.conditions.push(format!("{} <= ?", safe_column(column)));
        self.bindings.push(value.into());
        self
    }

    pub fn where_like(mut self, column: &str, pattern: impl Into<SqlValue>) -> Self {
        self.conditions.push(format!("{} LIKE ?", safe_column(column)));
        self.bindings.push(pattern.into());
        self
    }

    pub fn where_in<V: Into<SqlValue>>(mut self, column: &str, values: Vec<V>) -> Self {
        let placeholders: Vec<String> = values.iter().map(|_| "?".to_string()).collect();
        self.conditions.push(format!("{} IN ({})", safe_column(column), placeholders.join(", ")));
        for value in values {
            self.bindings.push(value.into());
        }
        self
    }

    pub fn where_not_in<V: Into<SqlValue>>(mut self, column: &str, values: Vec<V>) -> Self {
        let placeholders: Vec<String> = values.iter().map(|_| "?".to_string()).collect();
        self.conditions.push(format!("{} NOT IN ({})", safe_column(column), placeholders.join(", ")));
        for value in values {
            self.bindings.push(value.into());
        }
        self
    }

    pub fn where_between(mut self, column: &str, min: impl Into<SqlValue>, max: impl Into<SqlValue>) -> Self {
        self.conditions.push(format!("{} BETWEEN ? AND ?", safe_column(column)));
        self.bindings.push(min.into());
        self.bindings.push(max.into());
        self
    }

    pub fn where_null(mut self, column: &str) -> Self {
        self.conditions.push(format!("{} IS NULL", safe_column(column)));
        self
    }

    pub fn where_not_null(mut self, column: &str) -> Self {
        self.conditions.push(format!("{} IS NOT NULL", safe_column(column)));
        self
    }

    pub fn or(mut self, condition: &str) -> Self {
        self.conditions.push(format!("OR {}", condition));
        self
    }

    pub fn and(mut self, condition: &str) -> Self {
        self.conditions.push(format!("AND {}", condition));
        self
    }

    pub fn order_by(mut self, column: &str, direction: &str) -> Self {
        let direction = direction.to_uppercase();
        if direction == "ASC" || direction == "DESC" {
            self.orders.push(format!("{} {}", safe_column(column), direction));
        } else {
            self.orders.push(format!("{}", safe_column(column)));
        }
        self
    }

    pub fn limit(mut self, limit: u64) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn offset(mut self, offset: u64) -> Self {
        self.offset = Some(offset);
        self
    }

    pub fn build(&self) -> (String, Vec<SqlValue>) {
        let mut query = format!("SELECT * FROM {}", self.table_name);

        if !self.conditions.is_empty() {
            query.push_str(" WHERE ");
            query.push_str(&self.conditions.join(" AND "));
        }

        if !self.orders.is_empty() {
            query.push_str(" ORDER BY ");
            query.push_str(&self.orders.join(", "));
        }

        if let Some(limit) = self.limit {
            query.push_str(&format!(" LIMIT {}", limit));
        }

        if let Some(offset) = self.offset {
            query.push_str(&format!(" OFFSET {}", offset));
        }

        (query, self.bindings.clone())
    }
}

pub struct Query {
    table_name: String,
    where_conditions: Vec<WhereCondition>,
    orders: Vec<OrderClause>,
    pagination: Option<Pagination>,
    /// 最近一次构建或执行的操作（INSERT/UPDATE/DELETE/SELECT/COUNT）所生成的 SQL。
    /// 通过 [`Query::return_sql`] 可随时查看。
    /// 使用 `RwLock` 而非 `RefCell`，保证 `Query: Sync`（可在 async 上下文中共享）。
    last_sql: std::sync::RwLock<SqlStatement>,
}

#[derive(Clone)]
pub enum WhereCondition {
    Eq(String, SqlValue),
    Ne(String, SqlValue),
    Gt(String, SqlValue),
    Gte(String, SqlValue),
    Lt(String, SqlValue),
    Lte(String, SqlValue),
    Like(String, SqlValue),
    In(String, Vec<SqlValue>),
    NotIn(String, Vec<SqlValue>),
    Between(String, SqlValue, SqlValue),
    IsNull(String),
    IsNotNull(String),
    Or(Box<WhereCondition>),
    And(Box<WhereCondition>),
    /// 一组用 OR 连接的条件，整体作为 `(A OR B OR C)` 参与外部 AND 连接
    OrGroup(Vec<WhereCondition>),
    Raw(String),
}

#[derive(Clone)]
pub struct OrderClause {
    pub column: String,
    pub direction: OrderDirection,
}

#[derive(Debug, Clone)]
pub enum OrderDirection {
    Asc,
    Desc,
}

#[derive(Clone)]
pub struct Pagination {
    pub limit: u64,
    pub offset: u64,
}

/// 一个已绑定数据库连接的查询执行器，由 [`Query::query`] 返回。
///
/// 支持链式选择执行方式：
/// - [`QueryExecutor::count`]：执行 `SELECT COUNT(*)`，返回包含 `COUNT(*)` 列的结果集。
/// - [`QueryExecutor::select`]：执行 `SELECT *`，返回结果集。
pub struct QueryExecutor<'a> {
    query: &'a Query,
    db: &'a Database,
}

impl<'a> QueryExecutor<'a> {
    /// 执行 COUNT 查询，返回含 `COUNT(*)` 列的结果集。
    pub async fn count(&self) -> Result<QueryResult, DbError> {
        self.query.count().query(self.db).await
    }

    /// 执行 SELECT 查询，返回结果集。
    pub async fn select(&self) -> Result<QueryResult, DbError> {
        self.query.build().query(self.db).await
    }

    /// Dapper 风格：执行 SELECT 查询并把每行自动映射回模型类型 `M`。
    ///
    /// `M` 需实现 `Model`（通常由 `#[derive(Model)]` 自动生成 `from_row`）。
    ///
    /// ```ignore
    /// let users = Query::new("users")
    ///     .where_gt("age", 18)
    ///     .query(&db)
    ///     .models::<User>()
    ///     .await?;
    /// ```
    pub async fn models<M: Model>(&self) -> Result<Vec<M>, DbError> {
        self.query
            .build()
            .query(self.db)
            .await?
            .rows
            .iter()
            .map(|row| {
                M::from_row(row).ok_or_else(|| {
                    DbError::ParseError(format!(
                        "Failed to build {} from query row",
                        M::table_name()
                    ))
                })
            })
            .collect()
    }
}

impl Query {
    pub fn new(table_name: &str) -> Self {
        let table_name = safe_identifier(table_name);
        Self {
            table_name: table_name.clone(),
            where_conditions: Vec::new(),
            orders: Vec::new(),
            pagination: None,
            last_sql: std::sync::RwLock::new(SqlStatement::new(
                format!("SELECT * FROM {}", table_name),
                Vec::new(),
            )),
        }
    }

    pub fn where_eq(mut self, column: &str, value: impl Into<SqlValue>) -> Self {
        self.where_conditions.push(WhereCondition::Eq(column.to_string(), value.into()));
        self
    }

    pub fn where_ne(mut self, column: &str, value: impl Into<SqlValue>) -> Self {
        self.where_conditions.push(WhereCondition::Ne(column.to_string(), value.into()));
        self
    }

    pub fn where_gt(mut self, column: &str, value: impl Into<SqlValue>) -> Self {
        self.where_conditions.push(WhereCondition::Gt(column.to_string(), value.into()));
        self
    }

    pub fn where_gte(mut self, column: &str, value: impl Into<SqlValue>) -> Self {
        self.where_conditions.push(WhereCondition::Gte(column.to_string(), value.into()));
        self
    }

    pub fn where_lt(mut self, column: &str, value: impl Into<SqlValue>) -> Self {
        self.where_conditions.push(WhereCondition::Lt(column.to_string(), value.into()));
        self
    }

    pub fn where_lte(mut self, column: &str, value: impl Into<SqlValue>) -> Self {
        self.where_conditions.push(WhereCondition::Lte(column.to_string(), value.into()));
        self
    }

    pub fn where_like(mut self, column: &str, pattern: impl Into<SqlValue>) -> Self {
        self.where_conditions.push(WhereCondition::Like(column.to_string(), pattern.into()));
        self
    }

    pub fn where_in<V: Into<SqlValue>>(mut self, column: &str, values: Vec<V>) -> Self {
        self.where_conditions.push(WhereCondition::In(
            column.to_string(),
            values.into_iter().map(Into::into).collect(),
        ));
        self
    }

    pub fn where_not_in<V: Into<SqlValue>>(mut self, column: &str, values: Vec<V>) -> Self {
        self.where_conditions.push(WhereCondition::NotIn(
            column.to_string(),
            values.into_iter().map(Into::into).collect(),
        ));
        self
    }

    pub fn where_between(mut self, column: &str, min: impl Into<SqlValue>, max: impl Into<SqlValue>) -> Self {
        self.where_conditions.push(WhereCondition::Between(column.to_string(), min.into(), max.into()));
        self
    }

    pub fn where_null(mut self, column: &str) -> Self {
        self.where_conditions.push(WhereCondition::IsNull(column.to_string()));
        self
    }

    pub fn where_not_null(mut self, column: &str) -> Self {
        self.where_conditions.push(WhereCondition::IsNotNull(column.to_string()));
        self
    }

    pub fn where_raw(mut self, condition: &str) -> Self {
        self.where_conditions.push(WhereCondition::Raw(condition.to_string()));
        self
    }

    /// 一组用 OR 连接的条件，渲染为 `(A OR B)` 并与外部条件 AND 连接
    pub fn where_or(mut self, conditions: Vec<WhereCondition>) -> Self {
        self.where_conditions.push(WhereCondition::OrGroup(conditions));
        self
    }

    pub fn or(mut self, condition: WhereCondition) -> Self {
        self.where_conditions.push(WhereCondition::Or(Box::new(condition)));
        self
    }

    pub fn and(mut self, condition: WhereCondition) -> Self {
        self.where_conditions.push(WhereCondition::And(Box::new(condition)));
        self
    }

    pub fn order_by(mut self, column: &str, direction: OrderDirection) -> Self {
        self.orders.push(OrderClause {
            column: safe_column(column),
            direction,
        });
        self
    }

    pub fn order_by_asc(self, column: &str) -> Self {
        self.order_by(column, OrderDirection::Asc)
    }

    pub fn order_by_desc(self, column: &str) -> Self {
        self.order_by(column, OrderDirection::Desc)
    }

    pub fn limit(mut self, limit: u64) -> Self {
        self.pagination = Some(Pagination {
            limit,
            offset: self.pagination.as_ref().map_or(0, |p| p.offset),
        });
        self
    }

    pub fn offset(mut self, offset: u64) -> Self {
        self.pagination = Some(Pagination {
            limit: self.pagination.as_ref().map_or(u64::MAX, |p| p.limit),
            offset,
        });
        self
    }

    pub fn paginate(mut self, page: u64, per_page: u64) -> Self {
        self.pagination = Some(Pagination {
            limit: per_page,
            offset: (page - 1) * per_page,
        });
        self
    }

    pub fn build(&self) -> SqlStatement {
        let mut query = format!("SELECT * FROM {}", self.table_name);
        let mut bindings = Vec::new();

        if !self.where_conditions.is_empty() {
            query.push_str(" WHERE ");
            let mut where_clauses = Vec::new();
            for condition in &self.where_conditions {
                let (clause, mut b) = condition.to_sql();
                where_clauses.push(clause);
                bindings.append(&mut b);
            }
            query.push_str(&where_clauses.join(" AND "));
        }

        if !self.orders.is_empty() {
            query.push_str(" ORDER BY ");
            let order_clauses: Vec<String> = self
                .orders
                .iter()
                .map(|o| format!("{} {}", o.column, o.direction.as_str()))
                .collect();
            query.push_str(&order_clauses.join(", "));
        }

        if let Some(pag) = &self.pagination {
            query.push_str(&format!(" LIMIT {}", pag.limit));
            if pag.offset > 0 {
                query.push_str(&format!(" OFFSET {}", pag.offset));
            }
        }

        let stmt = SqlStatement::new(query, bindings);
        self.record(stmt.clone());
        stmt
    }

    pub fn count(&self) -> SqlStatement {
        let stmt = self.build_count_inner();
        self.record(stmt.clone());
        stmt
    }

    /// 仅构建 COUNT 语句（不执行），返回 `SqlStatement`，可用 `.return_sql()` 查看 SQL。
    pub fn build_count(&self) -> SqlStatement {
        let stmt = self.build_count_inner();
        self.record(stmt.clone());
        stmt
    }

    /// 直接执行 DELETE，返回受影响行数。可通过 [`Query::return_sql`] 查看 SQL。
    pub async fn delete(&self, db: &Database) -> Result<u64, DbError> {
        let stmt = self.build_delete();
        self.record(stmt.clone());
        stmt.execute(db).await
    }

    /// 直接执行 INSERT，返回受影响行数。可通过 [`Query::return_sql`] 查看 SQL。
    pub async fn insert(
        &self,
        columns: &[(&str, SqlValue)],
        db: &Database,
    ) -> Result<u64, DbError> {
        let stmt = self.build_insert(columns);
        self.record(stmt.clone());
        stmt.execute(db).await
    }

    /// 直接执行 UPDATE，返回受影响行数。可通过 [`Query::return_sql`] 查看 SQL。
    pub async fn update(
        &self,
        updates: &HashMap<String, SqlValue>,
        db: &Database,
    ) -> Result<u64, DbError> {
        let stmt = self.build_update(updates);
        self.record(stmt.clone());
        stmt.execute(db).await
    }

    /// 查看最近一次构建或执行的操作所生成的 SQL 与参数，返回 `(sql, params)`。
    pub fn return_sql(&self) -> (String, Vec<SqlValue>) {
        self.last_sql
            .read()
            .map(|s| s.return_sql())
            .unwrap_or_else(|_| (String::new(), Vec::new()))
    }

    /// 绑定一个数据库连接，返回一个可继续链式调用的执行器。
    ///
    /// 例如：`q.query(&db).count().await` 执行 COUNT 查询，
    /// `q.query(&db).select().await` 执行 SELECT 查询。
    pub fn query<'a>(&'a self, db: &'a Database) -> QueryExecutor<'a> {
        QueryExecutor { query: self, db }
    }

    fn record(&self, stmt: SqlStatement) {
        let Ok(mut guard) = self.last_sql.write() else {
            return;
        };
        *guard = stmt;
    }

    fn build_count_inner(&self) -> SqlStatement {
        let mut query = format!("SELECT COUNT(*) FROM {}", self.table_name);

        let mut bindings = Vec::new();

        if !self.where_conditions.is_empty() {
            query.push_str(" WHERE ");
            let mut where_clauses = Vec::new();
            for condition in &self.where_conditions {
                let (clause, mut b) = condition.to_sql();
                where_clauses.push(clause);
                bindings.append(&mut b);
            }
            query.push_str(&where_clauses.join(" AND "));
        }

        SqlStatement::new(query, bindings)
    }

    /// 仅构建 DELETE 语句（不执行），返回 `SqlStatement`，可用 `.return_sql()` 查看 SQL。
    pub fn build_delete(&self) -> SqlStatement {
        let mut query = format!("DELETE FROM {}", self.table_name);
        let mut bindings = Vec::new();

        if !self.where_conditions.is_empty() {
            query.push_str(" WHERE ");
            let mut where_clauses = Vec::new();
            for condition in &self.where_conditions {
                let (clause, mut b) = condition.to_sql();
                where_clauses.push(clause);
                bindings.append(&mut b);
            }
            query.push_str(&where_clauses.join(" AND "));
        }

        SqlStatement::new(query, bindings)
    }

    /// 仅构建 INSERT 语句（不执行），返回 `SqlStatement`，可用 `.return_sql()` 查看 SQL。
    pub fn build_insert(&self, columns: &[(&str, SqlValue)]) -> SqlStatement {
        let names: Vec<&str> = columns.iter().map(|(name, _)| *name).collect();
        let values: Vec<SqlValue> = columns.iter().map(|(_, value)| value.clone()).collect();
        let placeholders: Vec<String> = values.iter().map(|_| "?".to_string()).collect();
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            self.table_name,
            names.join(", "),
            placeholders.join(", ")
        );
        SqlStatement::new(sql, values)
    }

    /// 仅构建 UPDATE 语句（不执行），返回 `SqlStatement`，可用 `.return_sql()` 查看 SQL。
    pub fn build_update(&self, updates: &HashMap<String, SqlValue>) -> SqlStatement {
        let mut query = format!("UPDATE {} SET ", self.table_name);
        let mut bindings = Vec::new();

        let mut sorted_keys: Vec<&String> = updates.keys().collect();
        sorted_keys.sort();

        let mut set_clauses = Vec::new();
        for column in sorted_keys {
            set_clauses.push(format!("{} = ?", column));
            bindings.push(updates[column].clone());
        }
        query.push_str(&set_clauses.join(", "));

        if !self.where_conditions.is_empty() {
            query.push_str(" WHERE ");
            let mut where_clauses = Vec::new();
            for condition in &self.where_conditions {
                let (clause, mut b) = condition.to_sql();
                where_clauses.push(clause);
                bindings.append(&mut b);
            }
            query.push_str(&where_clauses.join(" AND "));
        }

        SqlStatement::new(query, bindings)
    }
}

impl WhereCondition {
    pub(crate) fn to_sql(&self) -> (String, Vec<SqlValue>) {
        match self {
            WhereCondition::Eq(col, val) => (format!("{} = ?", safe_column(col)), vec![val.clone()]),
            WhereCondition::Ne(col, val) => (format!("{} != ?", safe_column(col)), vec![val.clone()]),
            WhereCondition::Gt(col, val) => (format!("{} > ?", safe_column(col)), vec![val.clone()]),
            WhereCondition::Gte(col, val) => (format!("{} >= ?", safe_column(col)), vec![val.clone()]),
            WhereCondition::Lt(col, val) => (format!("{} < ?", safe_column(col)), vec![val.clone()]),
            WhereCondition::Lte(col, val) => (format!("{} <= ?", safe_column(col)), vec![val.clone()]),
            WhereCondition::Like(col, pat) => (format!("{} LIKE ?", safe_column(col)), vec![pat.clone()]),
            WhereCondition::In(col, vals) => {
                let placeholders: Vec<String> = vals.iter().map(|_| "?".to_string()).collect();
                (format!("{} IN ({})", safe_column(col), placeholders.join(", ")), vals.clone())
            }
            WhereCondition::NotIn(col, vals) => {
                let placeholders: Vec<String> = vals.iter().map(|_| "?".to_string()).collect();
                (
                    format!("{} NOT IN ({})", safe_column(col), placeholders.join(", ")),
                    vals.clone(),
                )
            }
            WhereCondition::Between(col, min, max) => {
                (format!("{} BETWEEN ? AND ?", safe_column(col)), vec![min.clone(), max.clone()])
            }
            WhereCondition::IsNull(col) => (format!("{} IS NULL", safe_column(col)), vec![]),
            WhereCondition::IsNotNull(col) => (format!("{} IS NOT NULL", safe_column(col)), vec![]),
            WhereCondition::Or(cond) => {
                let (sql, bindings) = cond.to_sql();
                (format!("OR ({})", sql), bindings)
            }
            WhereCondition::And(cond) => {
                let (sql, bindings) = cond.to_sql();
                (format!("AND ({})", sql), bindings)
            }
            WhereCondition::OrGroup(conds) => {
                let mut clauses = Vec::new();
                let mut bindings = Vec::new();
                for cond in conds {
                    let (clause, mut b) = cond.to_sql();
                    clauses.push(clause);
                    bindings.append(&mut b);
                }
                (format!("({})", clauses.join(" OR ")), bindings)
            }
            WhereCondition::Raw(raw) => (raw.clone(), vec![]),
        }
    }
}

impl OrderDirection {
    pub fn as_str(&self) -> &str {
        match self {
            OrderDirection::Asc => "ASC",
            OrderDirection::Desc => "DESC",
        }
    }
}

impl Default for Query {
    fn default() -> Self {
        Self::new("")
    }
}

// Example usage
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_builder_simple() {
        let builder = QueryBuilder::new("users")
            .where_eq("name", "John")
            .limit(10);

        let (query, bindings) = builder.build();
        assert_eq!(query, "SELECT * FROM users WHERE name = ? LIMIT 10");
        assert_eq!(bindings.len(), 1);
        assert!(matches!(bindings[0], SqlValue::String(_)));
    }

    #[test]
    fn test_query_builder_complex() {
        let builder = QueryBuilder::new("users")
            .where_eq("age", 25)
            .where_like("name", "John%")
            .order_by("created_at", "DESC")
            .limit(10)
            .offset(5);

        let (query, bindings) = builder.build();
        assert_eq!(query, "SELECT * FROM users WHERE age = ? AND name LIKE ? ORDER BY created_at DESC LIMIT 10 OFFSET 5");
        assert_eq!(bindings.len(), 2);
    }

    #[test]
    fn test_query_fluent() {
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
    fn test_query_fluent_auto_value_conversion() {
        // 无需手写 SqlValue::Type(...)，直接书写原生值即可自动转换。
        let query = Query::new("users")
            .where_eq("active", true)
            .where_gte("age", 18i64)
            .where_gt("score", 80.5f64)
            .where_in("id", vec![1u32, 2u32, 3u32])
            .where_like("name", "A%");

        let (sql, bindings) = query.build().return_sql();
        assert!(sql.contains("active = ?"));
        assert!(sql.contains("age >= ?"));
        assert!(sql.contains("score > ?"));
        assert!(sql.contains("id IN (?, ?, ?)"));
        assert!(sql.contains("name LIKE ?"));
        assert_eq!(bindings.len(), 7);
        assert!(matches!(bindings[0], SqlValue::Bool(true)));
        assert!(matches!(bindings[1], SqlValue::I64(18)));
        assert!(matches!(bindings[2], SqlValue::F64(v) if v == 80.5));
        assert!(matches!(bindings[3], SqlValue::I64(1)));
        assert!(matches!(bindings[4], SqlValue::I64(2)));
        assert!(matches!(bindings[5], SqlValue::I64(3)));
        assert!(matches!(bindings[6], SqlValue::String(_)));
    }

    #[test]
    fn test_query_with_pagination() {
        let query = Query::new("users").paginate(2, 10);

        let (sql, bindings) = query.build().return_sql();
        assert_eq!(sql, "SELECT * FROM users LIMIT 10 OFFSET 10");
        assert_eq!(bindings, Vec::<SqlValue>::new());
    }

    #[test]
    fn test_query_count() {
        let query = Query::new("users")
            .where_eq("status", "active")
            .where_gt("age", 18);

        let (sql, bindings) = query.count().return_sql();
        assert_eq!(sql, "SELECT COUNT(*) FROM users WHERE status = ? AND age > ?");
        assert_eq!(bindings.len(), 2);
    }

    #[test]
    fn test_query_update() {
        let query = Query::new("users").where_eq("id", "123");
        let mut updates = HashMap::new();
        updates.insert("name".to_string(), SqlValue::String("John Doe".to_string()));
        updates.insert("age".to_string(), SqlValue::I32(25));

        let (sql, bindings) = query.build_update(&updates).return_sql();
        // HashMap 迭代顺序不确定，但应包含所有 set 字段
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
    fn test_query_where_in() {
        let query = Query::new("users").where_in("id", vec![
            SqlValue::I32(1),
            SqlValue::I32(2),
            SqlValue::I32(3)
        ]);

        let (sql, bindings) = query.build().return_sql();
        assert_eq!(sql, "SELECT * FROM users WHERE id IN (?, ?, ?)");
        assert_eq!(bindings.len(), 3);
    }

    #[test]
    fn test_query_where_between() {
        let query = Query::new("users").where_between("age", 18, 65);

        let (sql, bindings) = query.build().return_sql();
        assert_eq!(sql, "SELECT * FROM users WHERE age BETWEEN ? AND ?");
        assert_eq!(bindings.len(), 2);
    }

    #[test]
    fn test_query_where_null() {
        let query = Query::new("users").where_null("deleted_at");

        let (sql, bindings) = query.build().return_sql();
        assert_eq!(sql, "SELECT * FROM users WHERE deleted_at IS NULL");
        assert_eq!(bindings, Vec::<SqlValue>::new());
    }

    #[test]
    fn test_sql_statement_return_sql() {
        let stmt = Query::new("users").where_eq("id", "1").build();
        let (sql, params) = stmt.return_sql();
        assert_eq!(sql, "SELECT * FROM users WHERE id = ?");
        assert_eq!(params, vec![SqlValue::String("1".to_string())]);
    }
}