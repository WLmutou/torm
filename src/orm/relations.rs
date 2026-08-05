use async_trait::async_trait;
use std::marker::PhantomData;
use crate::db::error::{Result, TormError};
use crate::orm::query::Query;
use crate::db::database::Database;
use crate::orm::model::Model;

/// 关联关系类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationType {
    BelongsTo,
    HasOne,
    HasMany,
    ManyToMany,
}

/// 关联关系定义
pub struct Relation<M: Model, R: Model> {
    pub relation_type: RelationType,
    pub foreign_key: &'static str,
    pub local_key: &'static str,
    pub join_table: Option<&'static str>,
    pub _marker: PhantomData<(M, R)>,
}

impl<M: Model, R: Model> Relation<M, R> {
    pub fn belongs_to(foreign_key: &'static str) -> Self {
        Self {
            relation_type: RelationType::BelongsTo,
            foreign_key,
            local_key: M::primary_key(),
            join_table: None,
            _marker: PhantomData,
        }
    }

    pub fn has_one(foreign_key: &'static str) -> Self {
        Self {
            relation_type: RelationType::HasOne,
            foreign_key,
            local_key: M::primary_key(),
            join_table: None,
            _marker: PhantomData,
        }
    }

    pub fn has_many(foreign_key: &'static str) -> Self {
        Self {
            relation_type: RelationType::HasMany,
            foreign_key,
            local_key: M::primary_key(),
            join_table: None,
            _marker: PhantomData,
        }
    }

    pub fn many_to_many(foreign_key: &'static str, join_table: &'static str) -> Self {
        Self {
            relation_type: RelationType::ManyToMany,
            foreign_key,
            local_key: M::primary_key(),
            join_table: Some(join_table),
            _marker: PhantomData,
        }
    }

    pub fn build_query(&self, local_id: &str) -> Query {
        match self.relation_type {
            RelationType::BelongsTo => {
                Query::new(R::table_name())
                    .where_eq(self.foreign_key, local_id)
                    .limit(1)
            }
            RelationType::HasOne => {
                Query::new(R::table_name())
                    .where_eq(self.foreign_key, local_id)
                    .limit(1)
            }
            RelationType::HasMany => {
                Query::new(R::table_name())
                    .where_eq(self.foreign_key, local_id)
            }
            RelationType::ManyToMany => {
                // For many-to-many, we need a join query
                // This is simplified - actual implementation would need JOIN support
                let join_table = self.join_table.unwrap_or("join_table");
                Query::new(R::table_name())
                    .where_eq(join_table, local_id) // Placeholder for join logic
            }
        }
    }
}

/// BelongsTo 关联
pub struct BelongsTo<M: Model, R: Model> {
    pub relation: Relation<M, R>,
}

impl<M: Model, R: Model> BelongsTo<M, R> {
    pub fn new(foreign_key: &'static str) -> Self {
        Self {
            relation: Relation::belongs_to(foreign_key),
        }
    }

    pub fn load(&self, local_id: &str) -> Query {
        self.relation.build_query(local_id)
    }
}

/// HasOne 关联
pub struct HasOne<M: Model, R: Model> {
    pub relation: Relation<M, R>,
}

impl<M: Model, R: Model> HasOne<M, R> {
    pub fn new(foreign_key: &'static str) -> Self {
        Self {
            relation: Relation::has_one(foreign_key),
        }
    }

    pub fn load(&self, local_id: &str) -> Query {
        self.relation.build_query(local_id)
    }
}

/// HasMany 关联
pub struct HasMany<M: Model, R: Model> {
    pub relation: Relation<M, R>,
}

impl<M: Model, R: Model> HasMany<M, R> {
    pub fn new(foreign_key: &'static str) -> Self {
        Self {
            relation: Relation::has_many(foreign_key),
        }
    }

    pub fn load(&self, local_id: &str) -> Query {
        self.relation.build_query(local_id)
    }
}

/// ManyToMany 关联
pub struct ManyToMany<M: Model, R: Model> {
    pub relation: Relation<M, R>,
}

impl<M: Model, R: Model> ManyToMany<M, R> {
    pub fn new(foreign_key: &'static str, join_table: &'static str) -> Self {
        Self {
            relation: Relation::many_to_many(foreign_key, join_table),
        }
    }

    pub fn load(&self, local_id: &str) -> Query {
        self.relation.build_query(local_id)
    }
}

/// 预加载 trait
#[async_trait]
pub trait Preload<M: Model> {
    async fn preload(&mut self, database: &Database) -> Result<()>;
}

/// 预加载查询构建器
pub struct PreloadBuilder {
    relations: Vec<String>,
    conditions: Vec<String>,
}

impl PreloadBuilder {
    pub fn new() -> Self {
        Self {
            relations: Vec::new(),
            conditions: Vec::new(),
        }
    }

    pub fn preload(mut self, relation: &str) -> Self {
        self.relations.push(relation.to_string());
        self
    }

    pub fn with(mut self, condition: &str) -> Self {
        self.conditions.push(condition.to_string());
        self
    }

    pub fn build(&self) -> (String, Vec<String>) {
        let mut query = "SELECT * FROM table_name".to_string();
        let mut bindings = Vec::new();

        // Add preload logic
        if !self.relations.is_empty() {
            query.push_str(" WITH ");
            for relation in &self.relations {
                query.push_str(relation);
                query.push_str(", ");
            }
            query = query.trim_end_matches(", ").to_string();
        }

        // Add conditions
        for condition in &self.conditions {
            query.push_str(" ");
            query.push_str(condition);
        }

        (query, bindings)
    }
}

impl Default for PreloadBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// 关联查询辅助函数
pub fn belongs_to<M: Model, R: Model>(foreign_key: &'static str) -> BelongsTo<M, R> {
    BelongsTo::new(foreign_key)
}

pub fn has_one<M: Model, R: Model>(foreign_key: &'static str) -> HasOne<M, R> {
    HasOne::new(foreign_key)
}

pub fn has_many<M: Model, R: Model>(foreign_key: &'static str) -> HasMany<M, R> {
    HasMany::new(foreign_key)
}

pub fn many_to_many<M: Model, R: Model>(foreign_key: &'static str, join_table: &'static str) -> ManyToMany<M, R> {
    ManyToMany::new(foreign_key, join_table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_belongs_to_relation() {
        let relation: Relation<User, Post> = Relation::belongs_to("user_id");
        assert_eq!(relation.relation_type, RelationType::BelongsTo);
        assert_eq!(relation.foreign_key, "user_id");
    }

    #[test]
    fn test_has_one_relation() {
        let relation: Relation<User, Profile> = Relation::has_one("user_id");
        assert_eq!(relation.relation_type, RelationType::HasOne);
        assert_eq!(relation.foreign_key, "user_id");
    }

    #[test]
    fn test_has_many_relation() {
        let relation: Relation<User, Post> = Relation::has_many("user_id");
        assert_eq!(relation.relation_type, RelationType::HasMany);
        assert_eq!(relation.foreign_key, "user_id");
    }

    #[test]
    fn test_many_to_many_relation() {
        let relation: Relation<User, Role> = Relation::many_to_many("role_id", "user_roles");
        assert_eq!(relation.relation_type, RelationType::ManyToMany);
        assert_eq!(relation.foreign_key, "role_id");
        assert_eq!(relation.join_table, Some("user_roles"));
    }

    #[test]
    fn test_preload_builder() {
        let builder = PreloadBuilder::new()
            .preload("posts")
            .preload("comments")
            .with("WHERE status = 'active'");

        let (sql, _) = builder.build();
        assert!(sql.contains("WITH"));
        assert!(sql.contains("posts"));
        assert!(sql.contains("comments"));
        assert!(sql.contains("WHERE status = 'active'"));
    }
}

// Test models
#[derive(Debug, Clone)]
pub struct User {
    pub id: String,
    pub name: String,
    pub timestamps: crate::orm::model::Timestamps,
}

#[async_trait::async_trait]
impl Model for User {
    fn table_name() -> &'static str {
        "users"
    }

    fn id(&self) -> Option<String> {
        Some(self.id.clone())
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

#[derive(Debug, Clone)]
pub struct Post {
    pub id: String,
    pub title: String,
    pub user_id: String,
    pub timestamps: crate::orm::model::Timestamps,
}

#[async_trait::async_trait]
impl Model for Post {
    fn table_name() -> &'static str {
        "posts"
    }

    fn id(&self) -> Option<String> {
        Some(self.id.clone())
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

#[derive(Debug, Clone)]
pub struct Profile {
    pub id: String,
    pub bio: String,
    pub user_id: String,
    pub timestamps: crate::orm::model::Timestamps,
}

#[async_trait::async_trait]
impl Model for Profile {
    fn table_name() -> &'static str {
        "profiles"
    }

    fn id(&self) -> Option<String> {
        Some(self.id.clone())
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

#[derive(Debug, Clone)]
pub struct Role {
    pub id: String,
    pub name: String,
    pub timestamps: crate::orm::model::Timestamps,
}

#[async_trait::async_trait]
impl Model for Role {
    fn table_name() -> &'static str {
        "roles"
    }

    fn id(&self) -> Option<String> {
        Some(self.id.clone())
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

#[derive(Debug, Clone)]
pub struct Comment {
    pub id: String,
    pub content: String,
    pub post_id: String,
    pub user_id: String,
    pub timestamps: crate::orm::model::Timestamps,
}

#[async_trait::async_trait]
impl Model for Comment {
    fn table_name() -> &'static str {
        "comments"
    }

    fn id(&self) -> Option<String> {
        Some(self.id.clone())
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