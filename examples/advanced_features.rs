use torm::*;
use chrono::Utc;

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("🚀 TORM 高级功能演示\n");

    // 1. 关联关系演示
    println!("🔗 关联关系演示");
    println!("========================");
    demonstrate_relations()?;
    println!();

    // 2. 高级查询演示
    println!("🔍 高级查询演示");
    println!("========================");
    demonstrate_advanced_queries()?;
    println!();

    // 3. 日志系统演示
    println!("📝 日志系统演示");
    println!("========================");
    demonstrate_logging()?;
    println!();

    // 4. 数据迁移演示
    println!("🗃️  数据迁移演示");
    println!("========================");
    demonstrate_migration()?;
    println!();

    // 5. 性能优化演示
    println!("⚡ 性能优化演示");
    println!("========================");
    demonstrate_performance()?;
    println!();

    println!("🎉 所有高级功能演示完成!");
    println!();
    println!("📚 TORM 完整功能总结:");
    println!("  ✅ 第一阶段 (MVP): 基础 CRUD、查询、事务、时间戳管理");
    println!("  ✅ 第二阶段: 关联关系、预加载、高级查询");
    println!("  ✅ 第三阶段: 数据迁移、日志系统、性能优化");
    println!("  ✅ 完整的测试套件");
    println!("  ✅ 详细的文档和示例");

    Ok(())
}

fn demonstrate_relations() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("用户和帖子的关联关系:");
    println!("  用户 (User) -[1]->-[N]- 帖子 (Post)");
    println!("  用户 (User) -[1]->-[1]- 档案 (Profile)");
    println!("  用户 (User) -[N]->-[N]- 角色 (Role)");
    
    println!();
    println!("定义关联关系:");
    
    // BelongsTo
    let posts_relation = torm::orm::relations::BelongsTo::<Post, User>::new("user_id");
    let query = posts_relation.load("user_123");
    let (sql, _) = query.build().return_sql();
    println!("  帖子属于用户: {}", sql);
    
    // HasMany
    let users_posts = torm::orm::relations::HasMany::<User, Post>::new("user_id");
    let query = users_posts.load("user_123");
    let (sql, _) = query.build().return_sql();
    println!("  用户有多个帖子: {}", sql);
    
    // ManyToMany
    let user_roles = torm::orm::relations::ManyToMany::<User, Role>::new("role_id", "user_roles");
    let query = user_roles.load("user_123");
    let (sql, _) = query.build().return_sql();
    println!("  用户有多个角色: {}", sql);

    println!();
    println!("预加载功能:");
    let preload = PreloadBuilder::new()
        .preload("posts")
        .preload("profile")
        .with("WHERE status = 'active'");
    let (sql, _) = preload.build();
    println!("  预加载 SQL: {}", sql);

    Ok(())
}

fn demonstrate_advanced_queries() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("复杂 JOIN 查询:");
    let (sql, _) = AdvancedQuery::new("users")
        .select(&["users.id", "users.name", "posts.title", "comments.content"])
        .inner_join("posts", "users.id = posts.user_id")
        .left_join("comments", "posts.id = comments.post_id")
        .where_eq("users.status", "active")
        .where_gt("posts.created_at", "2024-01-01")
        .order_by_desc("posts.created_at")
        .limit(20)
        .build_select();
    
    println!("  {}", sql);

    println!();
    println!("GROUP BY 和 HAVING:");
    let (sql, _) = AdvancedQuery::new("orders")
        .select(&["user_id", "COUNT(*) as order_count", "SUM(total) as total_spent"])
        .group_by(&["user_id"])
        .having(torm::orm::query::WhereCondition::Gte("COUNT(*)".to_string(), "5".into()))
        .order_by_desc("total_spent")
        .build_select();
    
    println!("  {}", sql);

    println!();
    println!("聚合函数:");
    let (sql, _) = AdvancedQuery::new("products")
        .count("*")
        .sum("price")
        .avg("rating")
        .min("price")
        .max("price")
        .build_select();
    
    println!("  {}", sql);

    println!();
    println!("DISTINCT 查询:");
    let (sql, _) = AdvancedQuery::new("users")
        .distinct()
        .select(&["country"])
        .order_by_asc("country")
        .build_select();
    
    println!("  {}", sql);

    println!();
    println!("UNION 查询:");
    let (sql, _) = AdvancedQuery::new("users")
        .select(&["id", "name", "email"])
        .where_eq("role", "admin")
        .union("SELECT id, name, email FROM super_admins")
        .order_by_asc("name")
        .build_select();
    
    println!("  {}", sql);

    Ok(())
}

fn demonstrate_logging() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("创建日志管理器:");
    
    // 组合日志器
    let composite_logger = CompositeLogger::new()
        .add_logger(Box::new(ConsoleLogger::new(LogLevel::Info)));
    
    let log_manager = LogManager::new(Box::new(composite_logger));
    
    println!("  日志级别设置:");
    log_manager.info("系统启动");
    log_manager.warn("配置文件不存在，使用默认配置");
    log_manager.error("连接数据库失败，正在重试");
    log_manager.debug("调试信息: SQL 查询已准备");

    println!();
    println!("查询追踪:");
    let tracer = log_manager.create_tracer("SELECT * FROM users WHERE status = 'active'");
    // 模拟查询执行
    std::thread::sleep(std::time::Duration::from_millis(50));
    tracer.finish();

    println!();
    println!("慢查询追踪:");
    let slow_tracer = log_manager.create_tracer("SELECT * FROM large_table");
    // 模拟慢查询
    std::thread::sleep(std::time::Duration::from_millis(1500));
    slow_tracer.finish();

    println!();
    println!("错误查询追踪:");
    let error_tracer = log_manager.create_tracer("SELECT * FROM non_existent_table");
    error_tracer.finish_with_error("Table 'non_existent_table' doesn't exist");

    println!();
    println!("性能监控:");
    let mut monitor = PerformanceMonitor::new(std::time::Duration::from_millis(1000));
    
    monitor.record_query(std::time::Duration::from_millis(50), true);
    monitor.record_query(std::time::Duration::from_millis(1500), true); // Slow query
    monitor.record_query(std::time::Duration::from_millis(75), false);  // Error
    monitor.record_query(std::time::Duration::from_millis(25), true);
    
    println!("  {}", monitor.get_stats());

    Ok(())
}

fn demonstrate_migration() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("创建用户表迁移:");
    let create_users_migration = Migration::new("create_users_table", 20240101000000)
        .add_operation(MigrationOperation::CreateTable(
            TableDefinition::new("users")
                .add_column(ColumnDefinition::new("id", ColumnType::Integer).primary_key().auto_increment())
                .add_column(ColumnDefinition::new("name", ColumnType::String).nullable(false))
                .add_column(ColumnDefinition::new("email", ColumnType::String).unique())
                .add_column(ColumnDefinition::new("created_at", ColumnType::DateTime).default("NOW()"))
                .add_column(ColumnDefinition::new("updated_at", ColumnType::DateTime).default("NOW()"))
                .add_index(IndexDefinition::new("idx_user_email", &["email"]).unique())
        ))
        .add_rollback_operation(MigrationOperation::DropTable("users".to_string()));

    println!("  迁移名称: {}", create_users_migration.name);
    println!("  迁移版本: {}", create_users_migration.version);
    println!("  操作数量: {}", create_users_migration.operations.len());

    println!();
    println!("添加新列迁移:");
    let add_bio_column_migration = Migration::new("add_bio_to_users", 20240101000001)
        .add_operation(MigrationOperation::AddColumn(
            "users".to_string(),
            ColumnDefinition::new("bio", ColumnType::Text).comment("用户简介")
        ))
        .add_rollback_operation(MigrationOperation::DropColumn(
            "users".to_string(),
            "bio".to_string()
        ));

    println!("  迁移名称: {}", add_bio_column_migration.name);
    println!("  操作: 添加 bio 列到 users 表");

    println!();
    println!("外键约束迁移:");
    let add_fk_migration = Migration::new("add_posts_user_fk", 20240101000002)
        .add_operation(MigrationOperation::AddForeignKey(
            "posts".to_string(),
            ForeignKeyDefinition::new("fk_posts_user_id", &["user_id"], "users", &["id"])
                .on_delete("CASCADE")
                .on_update("CASCADE")
        ))
        .add_rollback_operation(MigrationOperation::DropForeignKey(
            "posts".to_string(),
            "fk_posts_user_id".to_string()
        ));

    println!("  迁移名称: {}", add_fk_migration.name);
    println!("  操作: 为 posts 表添加 user_id 外键约束");

    println!();
    println!("迁移状态:");
    let status = MigrationStatus {
        total_migrations: 3,
        applied_count: 2,
        pending_count: 1,
        latest_version: Some(20240101000001),
    };
    println!("  总迁移数: {}", status.total_migrations);
    println!("  已应用: {}", status.applied_count);
    println!("  待应用: {}", status.pending_count);
    println!("  最新版本: {:?}", status.latest_version);

    Ok(())
}

fn demonstrate_performance() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("查询缓存:");
    let cache = QueryCache::new(100, Some(std::time::Duration::from_secs(3600)));
    
    cache.set("user:1", "用户数据1");
    cache.set("user:2", "用户数据2");
    cache.set("user:3", "用户数据3");
    
    println!("  缓存数据: user:1, user:2, user:3");
    println!("  获取 user:1: {:?}", cache.get("user:1"));
    println!("  获取 user:4: {:?}", cache.get("user:4"));
    
    let stats = cache.get_stats();
    println!("  缓存统计: 命中={}, 未命中={}, 命中率={:.1}%", 
        stats.hits, stats.misses, stats.hit_rate() * 100.0);

    println!();
    println!("批量操作:");
    let batch = BatchOperation::new(100)
        .add_create("新帖子1")
        .add_create("新帖子2")
        .add_create("新帖子3")
        .add_update("post_1".to_string(), "更新的帖子1")
        .add_delete("post_999".to_string());
    
    println!("  批量操作包含: {} 个操作", batch.len());
    println!("  是否为空: {}", batch.is_empty());
    println!("  是否已满: {}", batch.is_full());

    println!();
    println!("连接池监控:");
    let pool_monitor = ConnectionPoolMonitor::new(10, 2);
    
    // 模拟连接使用
    pool_monitor.record_active();
    pool_monitor.record_active();
    pool_monitor.record_idle();
    
    let pool_stats = pool_monitor.get_stats();
    println!("  活跃连接: {}", pool_stats.active_connections);
    println!("  空闲连接: {}", pool_stats.idle_connections);
    println!("  总连接数: {}", pool_stats.total_connections);
    println!("  利用率: {:.1}%", pool_stats.utilization_rate * 100.0);

    println!();
    println!("性能管理器:");
    let config = PerformanceConfig::default();
    let manager = PerformanceManager::new(config);
    
    let suggestions = manager.get_optimization_suggestions();
    println!("  优化建议数量: {}", suggestions.len());
    
    println!();
    println!("慢查询检测:");
    let slow_query_duration = std::time::Duration::from_millis(1500);
    let normal_query_duration = std::time::Duration::from_millis(100);
    
    println!("  1500ms 查询是否慢: {}", manager.is_slow_query(slow_query_duration));
    println!("  100ms 查询是否慢: {}", manager.is_slow_query(normal_query_duration));

    println!();
    println!("性能配置:");
    let config = PerformanceConfig::default();
    println!("  查询缓存启用: {}", config.query_cache_enabled);
    println!("  查询缓存大小: {}", config.query_cache_size);
    println!("  批量操作大小: {}", config.batch_size);
    println!("  连接池最大连接数: {}", config.connection_pool_max_size);
    println!("  慢查询阈值: {:?}", config.slow_query_threshold);

    Ok(())
}

// 示例模型结构
#[derive(Debug, Clone)]
pub struct User {
    pub id: String,
    pub name: String,
    pub timestamps: torm::orm::model::Timestamps,
}

#[async_trait::async_trait]
impl Model for User {
    fn table_name() -> &'static str { "users" }
    fn id(&self) -> Option<String> { Some(self.id.clone()) }
    fn set_id(&mut self, id: String) { self.id = id; }
    fn created_at(&self) -> Option<chrono::DateTime<Utc>> { self.timestamps.created_at }
    fn updated_at(&self) -> Option<chrono::DateTime<Utc>> { self.timestamps.updated_at }
    fn deleted_at(&self) -> Option<chrono::DateTime<Utc>> { self.timestamps.deleted_at }
    fn set_created_at(&mut self, timestamp: chrono::DateTime<Utc>) { self.timestamps.created_at = Some(timestamp); }
    fn set_updated_at(&mut self, timestamp: chrono::DateTime<Utc>) { self.timestamps.updated_at = Some(timestamp); }
    fn set_deleted_at(&mut self, timestamp: Option<chrono::DateTime<Utc>>) { self.timestamps.deleted_at = timestamp; }
}

#[derive(Debug, Clone)]
pub struct Post {
    pub id: String,
    pub title: String,
    pub user_id: String,
    pub timestamps: torm::orm::model::Timestamps,
}

#[async_trait::async_trait]
impl Model for Post {
    fn table_name() -> &'static str { "posts" }
    fn id(&self) -> Option<String> { Some(self.id.clone()) }
    fn set_id(&mut self, id: String) { self.id = id; }
    fn created_at(&self) -> Option<chrono::DateTime<Utc>> { self.timestamps.created_at }
    fn updated_at(&self) -> Option<chrono::DateTime<Utc>> { self.timestamps.updated_at }
    fn deleted_at(&self) -> Option<chrono::DateTime<Utc>> { self.timestamps.deleted_at }
    fn set_created_at(&mut self, timestamp: chrono::DateTime<Utc>) { self.timestamps.created_at = Some(timestamp); }
    fn set_updated_at(&mut self, timestamp: chrono::DateTime<Utc>) { self.timestamps.updated_at = Some(timestamp); }
    fn set_deleted_at(&mut self, timestamp: Option<chrono::DateTime<Utc>>) { self.timestamps.deleted_at = timestamp; }
}

#[derive(Debug, Clone)]
pub struct Profile {
    pub id: String,
    pub bio: String,
    pub user_id: String,
    pub timestamps: torm::orm::model::Timestamps,
}

#[async_trait::async_trait]
impl Model for Profile {
    fn table_name() -> &'static str { "profiles" }
    fn id(&self) -> Option<String> { Some(self.id.clone()) }
    fn set_id(&mut self, id: String) { self.id = id; }
    fn created_at(&self) -> Option<chrono::DateTime<Utc>> { self.timestamps.created_at }
    fn updated_at(&self) -> Option<chrono::DateTime<Utc>> { self.timestamps.updated_at }
    fn deleted_at(&self) -> Option<chrono::DateTime<Utc>> { self.timestamps.deleted_at }
    fn set_created_at(&mut self, timestamp: chrono::DateTime<Utc>) { self.timestamps.created_at = Some(timestamp); }
    fn set_updated_at(&mut self, timestamp: chrono::DateTime<Utc>) { self.timestamps.updated_at = Some(timestamp); }
    fn set_deleted_at(&mut self, timestamp: Option<chrono::DateTime<Utc>>) { self.timestamps.deleted_at = timestamp; }
}

#[derive(Debug, Clone)]
pub struct Role {
    pub id: String,
    pub name: String,
    pub timestamps: torm::orm::model::Timestamps,
}

#[async_trait::async_trait]
impl Model for Role {
    fn table_name() -> &'static str { "roles" }
    fn id(&self) -> Option<String> { Some(self.id.clone()) }
    fn set_id(&mut self, id: String) { self.id = id; }
    fn created_at(&self) -> Option<chrono::DateTime<Utc>> { self.timestamps.created_at }
    fn updated_at(&self) -> Option<chrono::DateTime<Utc>> { self.timestamps.updated_at }
    fn deleted_at(&self) -> Option<chrono::DateTime<Utc>> { self.timestamps.deleted_at }
    fn set_created_at(&mut self, timestamp: chrono::DateTime<Utc>) { self.timestamps.created_at = Some(timestamp); }
    fn set_updated_at(&mut self, timestamp: chrono::DateTime<Utc>) { self.timestamps.updated_at = Some(timestamp); }
    fn set_deleted_at(&mut self, timestamp: Option<chrono::DateTime<Utc>>) { self.timestamps.deleted_at = timestamp; }
}