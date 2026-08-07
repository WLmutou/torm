use crate::db::db_types::SqlValue;
use crate::orm::query::{OrderDirection, WhereCondition};
use crate::utils::sql_safety::{validate_identifier, validate_qualified_identifier};
use std::collections::HashMap;

/// 校验并规范化列/表达式标识符（允许 `users.id`、`COUNT(*)`）。
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
fn safe_table(id: &str) -> String {
    validate_qualified_identifier(id).unwrap_or_else(|e| {
        eprintln!("[torm::sql_safety] rejected unsafe table: {e}");
        String::new()
    })
}

/// 校验并规范化别名标识符（单一标识符，严格校验）。
fn safe_identifier(id: &str) -> String {
    validate_identifier(id).unwrap_or_else(|e| {
        eprintln!("[torm::sql_safety] rejected unsafe identifier: {e}");
        String::new()
    })
}

/// 高级查询构建器
pub struct AdvancedQuery {
    table_name: String,
    joins: Vec<JoinClause>,
    group_bys: Vec<String>,
    having: Option<WhereCondition>,
    distinct: bool,
    select_columns: Vec<String>,
    aggregations: Vec<AggregationClause>,
    unions: Vec<UnionClause>,
    where_conditions: Vec<WhereCondition>,
    orders: Vec<OrderClause>,
    pagination: Option<Pagination>,
}

#[derive(Debug, Clone)]
pub struct JoinClause {
    pub join_type: JoinType,
    pub table_name: String,
    pub alias: Option<String>,
    pub on_condition: String,
}

#[derive(Debug, Clone, Copy)]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
}

impl JoinType {
    pub fn as_str(&self) -> &str {
        match self {
            JoinType::Inner => "INNER JOIN",
            JoinType::Left => "LEFT JOIN",
            JoinType::Right => "RIGHT JOIN",
            JoinType::Full => "FULL JOIN",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AggregationClause {
    pub function: AggFunction,
    pub column: String,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum AggFunction {
    Count,
    Sum,
    Avg,
    Min,
    Max,
}

impl AggFunction {
    pub fn as_str(&self) -> &str {
        match self {
            AggFunction::Count => "COUNT",
            AggFunction::Sum => "SUM",
            AggFunction::Avg => "AVG",
            AggFunction::Min => "MIN",
            AggFunction::Max => "MAX",
        }
    }
}

#[derive(Debug, Clone)]
pub struct UnionClause {
    pub query: String,
    pub all: bool,
}

#[derive(Debug, Clone)]
pub struct OrderClause {
    pub column: String,
    pub direction: OrderDirection,
}

#[derive(Debug, Clone)]
pub struct Pagination {
    pub limit: u64,
    pub offset: u64,
}

impl AdvancedQuery {
    pub fn new(table_name: &str) -> Self {
        Self {
            table_name: safe_table(table_name),
            joins: Vec::new(),
            group_bys: Vec::new(),
            having: None,
            distinct: false,
            select_columns: Vec::new(),
            aggregations: Vec::new(),
            unions: Vec::new(),
            where_conditions: Vec::new(),
            orders: Vec::new(),
            pagination: None,
        }
    }

    /// SELECT 特定列
    pub fn select(mut self, columns: &[&str]) -> Self {
        self.select_columns = columns.iter().map(|s| safe_column(s)).collect();
        self
    }

    /// DISTINCT 查询
    pub fn distinct(mut self) -> Self {
        self.distinct = true;
        self
    }

    /// INNER JOIN
    pub fn inner_join(mut self, table_name: &str, on_condition: &str) -> Self {
        self.joins.push(JoinClause {
            join_type: JoinType::Inner,
            table_name: safe_table(table_name),
            alias: None,
            on_condition: on_condition.to_string(),
        });
        self
    }

    /// LEFT JOIN
    pub fn left_join(mut self, table_name: &str, on_condition: &str) -> Self {
        self.joins.push(JoinClause {
            join_type: JoinType::Left,
            table_name: safe_table(table_name),
            alias: None,
            on_condition: on_condition.to_string(),
        });
        self
    }

    /// RIGHT JOIN
    pub fn right_join(mut self, table_name: &str, on_condition: &str) -> Self {
        self.joins.push(JoinClause {
            join_type: JoinType::Right,
            table_name: safe_table(table_name),
            alias: None,
            on_condition: on_condition.to_string(),
        });
        self
    }

    /// FULL JOIN
    pub fn full_join(mut self, table_name: &str, on_condition: &str) -> Self {
        self.joins.push(JoinClause {
            join_type: JoinType::Full,
            table_name: safe_table(table_name),
            alias: None,
            on_condition: on_condition.to_string(),
        });
        self
    }

    /// JOIN 带别名
    pub fn join_alias(mut self, join_type: JoinType, table_name: &str, alias: &str, on_condition: &str) -> Self {
        self.joins.push(JoinClause {
            join_type,
            table_name: safe_table(table_name),
            alias: Some(safe_identifier(alias)),
            on_condition: on_condition.to_string(),
        });
        self
    }

    /// GROUP BY
    pub fn group_by(mut self, columns: &[&str]) -> Self {
        self.group_bys = columns.iter().map(|s| safe_column(s)).collect();
        self
    }

    /// HAVING
    pub fn having(mut self, condition: WhereCondition) -> Self {
        self.having = Some(condition);
        self
    }

    /// 聚合函数
    pub fn count(mut self, column: &str) -> Self {
        self.aggregations.push(AggregationClause {
            function: AggFunction::Count,
            column: safe_column(column),
            alias: None,
        });
        self
    }

    pub fn sum(mut self, column: &str) -> Self {
        self.aggregations.push(AggregationClause {
            function: AggFunction::Sum,
            column: safe_column(column),
            alias: None,
        });
        self
    }

    pub fn avg(mut self, column: &str) -> Self {
        self.aggregations.push(AggregationClause {
            function: AggFunction::Avg,
            column: safe_column(column),
            alias: None,
        });
        self
    }

    pub fn min(mut self, column: &str) -> Self {
        self.aggregations.push(AggregationClause {
            function: AggFunction::Min,
            column: safe_column(column),
            alias: None,
        });
        self
    }

    pub fn max(mut self, column: &str) -> Self {
        self.aggregations.push(AggregationClause {
            function: AggFunction::Max,
            column: safe_column(column),
            alias: None,
        });
        self
    }

    /// UNION
    pub fn union(mut self, query: &str) -> Self {
        self.unions.push(UnionClause {
            query: query.to_string(),
            all: false,
        });
        self
    }

    /// UNION ALL
    pub fn union_all(mut self, query: &str) -> Self {
        self.unions.push(UnionClause {
            query: query.to_string(),
            all: true,
        });
        self
    }

    /// WHERE 条件
    pub fn where_eq(mut self, column: &str, value: impl Into<SqlValue>) -> Self {
        self.where_conditions.push(WhereCondition::Eq(column.to_string(), value.into()));
        self
    }

    pub fn where_gt(mut self, column: &str, value: impl Into<SqlValue>) -> Self {
        self.where_conditions.push(WhereCondition::Gt(column.to_string(), value.into()));
        self
    }

    pub fn where_lt(mut self, column: &str, value: impl Into<SqlValue>) -> Self {
        self.where_conditions.push(WhereCondition::Lt(column.to_string(), value.into()));
        self
    }

    pub fn where_ne(mut self, column: &str, value: impl Into<SqlValue>) -> Self {
        self.where_conditions.push(WhereCondition::Ne(column.to_string(), value.into()));
        self
    }

    pub fn where_like(mut self, column: &str, pattern: impl Into<SqlValue>) -> Self {
        self.where_conditions.push(WhereCondition::Like(column.to_string(), pattern.into()));
        self
    }

    pub fn where_in(mut self, column: &str, values: Vec<SqlValue>) -> Self {
        self.where_conditions.push(WhereCondition::In(
            column.to_string(),
            values,
        ));
        self
    }

    pub fn where_between(mut self, column: &str, min: impl Into<SqlValue>, max: impl Into<SqlValue>) -> Self {
        self.where_conditions.push(WhereCondition::Between(
            column.to_string(),
            min.into(),
            max.into(),
        ));
        self
    }

    pub fn where_null(mut self, column: &str) -> Self {
        self.where_conditions.push(WhereCondition::IsNull(column.to_string()));
        self
    }

    pub fn where_raw(mut self, condition: &str) -> Self {
        self.where_conditions.push(WhereCondition::Raw(condition.to_string()));
        self
    }

    /// ORDER BY
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

    /// LIMIT 和 OFFSET
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

    /// 构建 SELECT 查询
    pub fn build_select(&self) -> (String, Vec<SqlValue>) {
        let mut query = String::new();
        let mut bindings: Vec<SqlValue> = Vec::new();

        // SELECT clause
        query.push_str("SELECT ");
        
        if self.distinct {
            query.push_str("DISTINCT ");
        }

        if !self.select_columns.is_empty() {
            query.push_str(&self.select_columns.join(", "));
        } else if !self.aggregations.is_empty() {
            let agg_parts: Vec<String> = self.aggregations.iter()
                .map(|agg| {
                    let alias = agg.alias.as_ref()
                        .map(|a| format!(" AS {}", a))
                        .unwrap_or_default();
                    format!("{}({}){}", agg.function.as_str(), agg.column, alias)
                })
                .collect();
            query.push_str(&agg_parts.join(", "));
        } else {
            query.push_str("*");
        }

        // FROM clause
        query.push_str(&format!(" FROM {}", self.table_name));

        // JOINs
        for join in &self.joins {
            query.push_str(&format!(" {} {}", join.join_type.as_str(), join.table_name));
            if let Some(alias) = &join.alias {
                query.push_str(&format!(" AS {}", alias));
            }
            query.push_str(&format!(" ON {}", join.on_condition));
        }

        // WHERE clause
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

        // GROUP BY
        if !self.group_bys.is_empty() {
            query.push_str(&format!(" GROUP BY {}", self.group_bys.join(", ")));
        }

        // HAVING
        if let Some(condition) = &self.having {
            let (clause, mut b) = condition.to_sql();
            query.push_str(&format!(" HAVING {}", clause));
            bindings.append(&mut b);
        }

        // ORDER BY
        if !self.orders.is_empty() {
            let order_parts: Vec<String> = self.orders.iter()
                .map(|o| format!("{} {}", o.column, o.direction.as_str()))
                .collect();
            query.push_str(&format!(" ORDER BY {}", order_parts.join(", ")));
        }

        // LIMIT and OFFSET
        if let Some(pag) = &self.pagination {
            query.push_str(&format!(" LIMIT {}", pag.limit));
            if pag.offset > 0 {
                query.push_str(&format!(" OFFSET {}", pag.offset));
            }
        }

        // UNIONs
        for union_clause in &self.unions {
            let union_type = if union_clause.all { "UNION ALL" } else { "UNION" };
            query.push_str(&format!(" {} ({})", union_type, union_clause.query));
        }

        (query, bindings)
    }

    /// 构建 COUNT 查询
    pub fn build_count(&self) -> (String, Vec<SqlValue>) {
        let mut query = format!("SELECT COUNT(*) FROM {}", self.table_name);
        let mut bindings: Vec<SqlValue> = Vec::new();

        // JOINs
        for join in &self.joins {
            query.push_str(&format!(" {} {}", join.join_type.as_str(), join.table_name));
            query.push_str(&format!(" ON {}", join.on_condition));
        }

        // WHERE clause
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

        // GROUP BY
        if !self.group_bys.is_empty() {
            query.push_str(&format!(" GROUP BY {}", self.group_bys.join(", ")));
        }

        (query, bindings)
    }

    /// 构建 UPDATE 查询
    pub fn build_update(&self, updates: &HashMap<String, SqlValue>) -> (String, Vec<SqlValue>) {
        let mut query = format!("UPDATE {} SET ", self.table_name);
        let mut bindings: Vec<SqlValue> = Vec::new();

        let mut set_clauses = Vec::new();
        for (column, value) in updates {
            set_clauses.push(format!("{} = ?", safe_column(column)));
            bindings.push(value.clone());
        }
        query.push_str(&set_clauses.join(", "));

        // WHERE clause
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

        (query, bindings)
    }

    /// 构建 DELETE 查询
    pub fn build_delete(&self) -> (String, Vec<SqlValue>) {
        let mut query = format!("DELETE FROM {}", self.table_name);
        let mut bindings: Vec<SqlValue> = Vec::new();

        // WHERE clause
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

        (query, bindings)
    }
}

impl Default for AdvancedQuery {
    fn default() -> Self {
        Self::new("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orm::query::WhereCondition;

    #[test]
    fn test_inner_join() {
        let query = AdvancedQuery::new("users")
            .select(&["users.id", "users.name", "posts.title"])
            .inner_join("posts", "users.id = posts.user_id")
            .where_eq("users.status", "active");

        let (sql, _) = query.build_select();
        assert!(sql.contains("SELECT users.id, users.name, posts.title"));
        assert!(sql.contains("INNER JOIN posts"));
        assert!(sql.contains("ON users.id = posts.user_id"));
        assert!(sql.contains("WHERE users.status = ?"));
    }

    #[test]
    fn test_left_join() {
        let query = AdvancedQuery::new("users")
            .left_join("posts", "users.id = posts.user_id")
            .where_null("posts.id");

        let (sql, _) = query.build_select();
        assert!(sql.contains("LEFT JOIN posts"));
        assert!(sql.contains("ON users.id = posts.user_id"));
        assert!(sql.contains("posts.id IS NULL"));
    }

    #[test]
    fn test_group_by_and_having() {
        let query = AdvancedQuery::new("orders")
            .select(&["user_id", "COUNT(*) as order_count", "SUM(total) as total_amount"])
            .group_by(&["user_id"])
            .having(WhereCondition::Gt("COUNT(*)".to_string(), SqlValue::String("5".to_string())));

        let (sql, _) = query.build_select();
        assert!(sql.contains("SELECT user_id, COUNT(*) as order_count, SUM(total) as total_amount"));
        assert!(sql.contains("GROUP BY user_id"));
        assert!(sql.contains("HAVING COUNT(*) > ?"));
    }

    #[test]
    fn test_aggregation_functions() {
        let query = AdvancedQuery::new("products")
            .count("*")
            .sum("price")
            .avg("rating")
            .min("price")
            .max("price");

        let (sql, _) = query.build_select();
        assert!(sql.contains("SELECT"));
        assert!(sql.contains("COUNT(*)"));
        assert!(sql.contains("SUM(price)"));
        assert!(sql.contains("AVG(rating)"));
        assert!(sql.contains("MIN(price)"));
        assert!(sql.contains("MAX(price)"));
    }

    #[test]
    fn test_distinct_query() {
        let query = AdvancedQuery::new("orders")
            .distinct()
            .select(&["user_id"])
            .where_eq("status", "completed");

        let (sql, _) = query.build_select();
        assert!(sql.contains("SELECT DISTINCT user_id"));
    }

    #[test]
    fn test_union() {
        let query = AdvancedQuery::new("users")
            .union("SELECT id, name FROM admins")
            .where_eq("status", "active");

        let (sql, _) = query.build_select();
        assert!(sql.contains("UNION"));
        assert!(sql.contains("SELECT id, name FROM admins"));
    }

    #[test]
    fn test_union_all() {
        let query = AdvancedQuery::new("users")
            .union_all("SELECT id, name FROM admins");

        let (sql, _) = query.build_select();
        assert!(sql.contains("UNION ALL"));
    }

    #[test]
    fn test_complex_join_with_multiple_conditions() {
        let query = AdvancedQuery::new("users")
            .select(&["users.id", "users.name", "orders.order_id", "orders.total"])
            .inner_join("orders", "users.id = orders.user_id")
            .inner_join("order_items", "orders.order_id = order_items.order_id")
            .where_gt("orders.total", "100")
            .where_eq("orders.status", "completed")
            .order_by_desc("orders.created_at");

        let (sql, _) = query.build_select();
        assert!(sql.contains("INNER JOIN orders"));
        assert!(sql.contains("INNER JOIN order_items"));
        assert!(sql.contains("orders.total > ?"));
        assert!(sql.contains("orders.status = ?"));
        assert!(sql.contains("ORDER BY orders.created_at DESC"));
    }

    #[test]
    fn test_join_with_alias() {
        let query = AdvancedQuery::new("users")
            .join_alias(JoinType::Left, "user_profiles", "profiles", "users.id = profiles.user_id")
            .select(&["users.id", "profiles.bio"]);

        let (sql, _) = query.build_select();
        assert!(sql.contains("LEFT JOIN user_profiles AS profiles"));
        assert!(sql.contains("ON users.id = profiles.user_id"));
    }

    #[test]
    fn test_group_by_multiple_columns() {
        let query = AdvancedQuery::new("sales")
            .select(&["category", "region", "SUM(amount) as total_sales"])
            .group_by(&["category", "region"])
            .order_by_desc("total_sales");

        let (sql, _) = query.build_select();
        assert!(sql.contains("GROUP BY category, region"));
        assert!(sql.contains("ORDER BY total_sales DESC"));
    }

    #[test]
    fn test_count_with_joins() {
        let query = AdvancedQuery::new("users")
            .inner_join("posts", "users.id = posts.user_id")
            .where_eq("posts.status", "published");

        let (sql, _) = query.build_count();
        assert!(sql.contains("SELECT COUNT(*) FROM users"));
        assert!(sql.contains("INNER JOIN posts"));
        assert!(sql.contains("posts.status = ?"));
    }

    #[test]
    fn test_update_with_joins() {
        let query = AdvancedQuery::new("users")
            .inner_join("orders", "users.id = orders.user_id")
            .where_eq("orders.status", "pending");

        let mut updates = HashMap::new();
        updates.insert("users.status".to_string(), SqlValue::String("inactive".to_string()));

        let (sql, _) = query.build_update(&updates);
        assert!(sql.contains("UPDATE users SET"));
        assert!(sql.contains("users.status = ?"));
        assert!(sql.contains("WHERE"));
    }

    #[test]
    fn test_delete_with_multiple_conditions() {
        let query = AdvancedQuery::new("users")
            .where_lt("created_at", "2023-01-01")
            .where_eq("status", "inactive")
            .where_null("last_login");

        let (sql, _) = query.build_delete();
        assert!(sql.contains("DELETE FROM users"));
        assert!(sql.contains("WHERE"));
        assert!(sql.contains("created_at < ?"));
        assert!(sql.contains("status = ?"));
        assert!(sql.contains("last_login IS NULL"));
    }
}