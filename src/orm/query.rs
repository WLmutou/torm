use crate::db::db_types::SqlValue;
use std::collections::HashMap;

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
            table_name: table_name.to_string(),
            conditions: Vec::new(),
            orders: Vec::new(),
            limit: None,
            offset: None,
            bindings: Vec::new(),
        }
    }

    pub fn where_eq(mut self, column: &str, value: impl Into<SqlValue>) -> Self {
        self.conditions.push(format!("{} = ?", column));
        self.bindings.push(value.into());
        self
    }

    pub fn where_ne(mut self, column: &str, value: impl Into<SqlValue>) -> Self {
        self.conditions.push(format!("{} != ?", column));
        self.bindings.push(value.into());
        self
    }

    pub fn where_gt(mut self, column: &str, value: impl Into<SqlValue>) -> Self {
        self.conditions.push(format!("{} > ?", column));
        self.bindings.push(value.into());
        self
    }

    pub fn where_gte(mut self, column: &str, value: impl Into<SqlValue>) -> Self {
        self.conditions.push(format!("{} >= ?", column));
        self.bindings.push(value.into());
        self
    }

    pub fn where_lt(mut self, column: &str, value: impl Into<SqlValue>) -> Self {
        self.conditions.push(format!("{} < ?", column));
        self.bindings.push(value.into());
        self
    }

    pub fn where_lte(mut self, column: &str, value: impl Into<SqlValue>) -> Self {
        self.conditions.push(format!("{} <= ?", column));
        self.bindings.push(value.into());
        self
    }

    pub fn where_like(mut self, column: &str, pattern: impl Into<SqlValue>) -> Self {
        self.conditions.push(format!("{} LIKE ?", column));
        self.bindings.push(pattern.into());
        self
    }

    pub fn where_in(mut self, column: &str, values: Vec<SqlValue>) -> Self {
        let placeholders: Vec<String> = values.iter().map(|_| "?".to_string()).collect();
        self.conditions.push(format!("{} IN ({})", column, placeholders.join(", ")));
        for value in values {
            self.bindings.push(value);
        }
        self
    }

    pub fn where_not_in(mut self, column: &str, values: Vec<SqlValue>) -> Self {
        let placeholders: Vec<String> = values.iter().map(|_| "?".to_string()).collect();
        self.conditions.push(format!("{} NOT IN ({})", column, placeholders.join(", ")));
        for value in values {
            self.bindings.push(value);
        }
        self
    }

    pub fn where_between(mut self, column: &str, min: impl Into<SqlValue>, max: impl Into<SqlValue>) -> Self {
        self.conditions.push(format!("{} BETWEEN ? AND ?", column));
        self.bindings.push(min.into());
        self.bindings.push(max.into());
        self
    }

    pub fn where_null(mut self, column: &str) -> Self {
        self.conditions.push(format!("{} IS NULL", column));
        self
    }

    pub fn where_not_null(mut self, column: &str) -> Self {
        self.conditions.push(format!("{} IS NOT NULL", column));
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
            self.orders.push(format!("{} {}", column, direction));
        } else {
            self.orders.push(format!("{}", column));
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

impl Query {
    pub fn new(table_name: &str) -> Self {
        Self {
            table_name: table_name.to_string(),
            where_conditions: Vec::new(),
            orders: Vec::new(),
            pagination: None,
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

    pub fn where_in(mut self, column: &str, values: Vec<SqlValue>) -> Self {
        self.where_conditions.push(WhereCondition::In(column.to_string(), values));
        self
    }

    pub fn where_not_in(mut self, column: &str, values: Vec<SqlValue>) -> Self {
        self.where_conditions.push(WhereCondition::NotIn(column.to_string(), values));
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
            column: column.to_string(),
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

    pub fn build(&self) -> (String, Vec<SqlValue>) {
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

        (query, bindings)
    }

    pub fn count(&self) -> (String, Vec<SqlValue>) {
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

        (query, bindings)
    }

    pub fn delete(&self) -> (String, Vec<SqlValue>) {
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

        (query, bindings)
    }

    pub fn update(&self, updates: &HashMap<String, SqlValue>) -> (String, Vec<SqlValue>) {
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

        (query, bindings)
    }
}

impl WhereCondition {
    pub(crate) fn to_sql(&self) -> (String, Vec<SqlValue>) {
        match self {
            WhereCondition::Eq(col, val) => (format!("{} = ?", col), vec![val.clone()]),
            WhereCondition::Ne(col, val) => (format!("{} != ?", col), vec![val.clone()]),
            WhereCondition::Gt(col, val) => (format!("{} > ?", col), vec![val.clone()]),
            WhereCondition::Gte(col, val) => (format!("{} >= ?", col), vec![val.clone()]),
            WhereCondition::Lt(col, val) => (format!("{} < ?", col), vec![val.clone()]),
            WhereCondition::Lte(col, val) => (format!("{} <= ?", col), vec![val.clone()]),
            WhereCondition::Like(col, pat) => (format!("{} LIKE ?", col), vec![pat.clone()]),
            WhereCondition::In(col, vals) => {
                let placeholders: Vec<String> = vals.iter().map(|_| "?".to_string()).collect();
                (format!("{} IN ({})", col, placeholders.join(", ")), vals.clone())
            }
            WhereCondition::NotIn(col, vals) => {
                let placeholders: Vec<String> = vals.iter().map(|_| "?".to_string()).collect();
                (
                    format!("{} NOT IN ({})", col, placeholders.join(", ")),
                    vals.clone(),
                )
            }
            WhereCondition::Between(col, min, max) => {
                (format!("{} BETWEEN ? AND ?", col), vec![min.clone(), max.clone()])
            }
            WhereCondition::IsNull(col) => (format!("{} IS NULL", col), vec![]),
            WhereCondition::IsNotNull(col) => (format!("{} IS NOT NULL", col), vec![]),
            WhereCondition::Or(cond) => {
                let (sql, bindings) = cond.to_sql();
                (format!("OR ({})", sql), bindings)
            }
            WhereCondition::And(cond) => {
                let (sql, bindings) = cond.to_sql();
                (format!("AND ({})", sql), bindings)
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

        let (sql, bindings) = query.build();
        assert_eq!(sql, "SELECT * FROM users WHERE status = ? AND age > ? ORDER BY created_at DESC LIMIT 20");
        assert_eq!(bindings.len(), 2);
    }

    #[test]
    fn test_query_with_pagination() {
        let query = Query::new("users").paginate(2, 10);

        let (sql, bindings) = query.build();
        assert_eq!(sql, "SELECT * FROM users LIMIT 10 OFFSET 10");
        assert_eq!(bindings, Vec::<SqlValue>::new());
    }

    #[test]
    fn test_query_count() {
        let query = Query::new("users")
            .where_eq("status", "active")
            .where_gt("age", 18);

        let (sql, bindings) = query.count();
        assert_eq!(sql, "SELECT COUNT(*) FROM users WHERE status = ? AND age > ?");
        assert_eq!(bindings.len(), 2);
    }

    #[test]
    fn test_query_update() {
        let query = Query::new("users").where_eq("id", "123");
        let mut updates = HashMap::new();
        updates.insert("name".to_string(), SqlValue::String("John Doe".to_string()));
        updates.insert("age".to_string(), SqlValue::I32(25));

        let (sql, bindings) = query.update(&updates);
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

        let (sql, bindings) = query.delete();
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

        let (sql, bindings) = query.build();
        assert_eq!(sql, "SELECT * FROM users WHERE id IN (?, ?, ?)");
        assert_eq!(bindings.len(), 3);
    }

    #[test]
    fn test_query_where_between() {
        let query = Query::new("users").where_between("age", 18, 65);

        let (sql, bindings) = query.build();
        assert_eq!(sql, "SELECT * FROM users WHERE age BETWEEN ? AND ?");
        assert_eq!(bindings.len(), 2);
    }

    #[test]
    fn test_query_where_null() {
        let query = Query::new("users").where_null("deleted_at");

        let (sql, bindings) = query.build();
        assert_eq!(sql, "SELECT * FROM users WHERE deleted_at IS NULL");
        assert_eq!(bindings, Vec::<SqlValue>::new());
    }
}