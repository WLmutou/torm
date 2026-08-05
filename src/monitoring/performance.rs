use std::collections::HashMap;
use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};
use serde::{Serialize, Deserialize};
use crate::utils::simple_lru::SimpleLruCache;

/// 查询缓存项
#[derive(Debug, Clone)]
pub struct CacheEntry<T> {
    pub data: T,
    pub created_at: Instant,
    pub expires_at: Option<Instant>,
    pub hit_count: u64,
}

impl<T> CacheEntry<T> {
    pub fn new(data: T, ttl: Option<Duration>) -> Self {
        let now = Instant::now();
        Self {
            data,
            created_at: now,
            expires_at: ttl.map(|ttl| now + ttl),
            hit_count: 0,
        }
    }

    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Instant::now() > expires_at
        } else {
            false
        }
    }

    pub fn increment_hit(&mut self) {
        self.hit_count += 1;
    }
}

/// 查询缓存（使用简化的 LRU 实现）
pub struct QueryCache<T: Clone> {
    cache: Arc<Mutex<SimpleLruCache<String, CacheEntry<T>>>>,
    default_ttl: Option<Duration>,
    stats: Arc<Mutex<CacheStats>>,
}

impl<T: Clone> QueryCache<T> {
    pub fn new(capacity: usize, default_ttl: Option<Duration>) -> Self {
        Self {
            cache: Arc::new(Mutex::new(SimpleLruCache::new(capacity))),
            default_ttl,
            stats: Arc::new(Mutex::new(CacheStats::default())),
        }
    }

    pub fn get(&self, key: &str) -> Option<T> {
        let mut cache = self.cache.lock().unwrap();
        let key_string = key.to_string();
        
        if let Some(entry) = cache.get(&key_string) {
            if !entry.is_expired() {
                // Create new entry with incremented hit count
                let mut updated_entry = entry.clone();
                updated_entry.increment_hit();
                cache.put(key_string.clone(), updated_entry);
                
                let mut stats = self.stats.lock().unwrap();
                stats.hits += 1;
                return Some(entry.data.clone());
            } else {
                // Remove expired entry
                cache.remove(&key_string);
            }
        }

        let mut stats = self.stats.lock().unwrap();
        stats.misses += 1;
        None
    }

    pub fn set(&self, key: &str, value: T) {
        let mut cache = self.cache.lock().unwrap();
        let ttl = self.default_ttl;
        cache.put(key.to_string(), CacheEntry::new(value, ttl));
        
        let mut stats = self.stats.lock().unwrap();
        stats.sets += 1;
    }

    pub fn set_with_ttl(&self, key: &str, value: T, ttl: Duration) {
        let mut cache = self.cache.lock().unwrap();
        cache.put(key.to_string(), CacheEntry::new(value, Some(ttl)));
        
        let mut stats = self.stats.lock().unwrap();
        stats.sets += 1;
    }

    pub fn remove(&self, key: &str) {
        let mut cache = self.cache.lock().unwrap();
        cache.remove(&key.to_string());
        
        let mut stats = self.stats.lock().unwrap();
        stats.deletes += 1;
    }

    pub fn clear(&self) {
        let mut cache = self.cache.lock().unwrap();
        cache.clear();
        
        let mut stats = self.stats.lock().unwrap();
        stats.clears += 1;
    }

    pub fn get_stats(&self) -> CacheStats {
        let stats = self.stats.lock().unwrap();
        stats.clone()
    }

    pub fn invalidate_prefix(&self, prefix: &str) {
        let mut cache = self.cache.lock().unwrap();
        let keys_to_remove: Vec<String> = cache.keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect();
        
        for key in keys_to_remove {
            cache.remove(&key);
        }
    }

    pub fn cleanup_expired(&self) {
        let mut cache = self.cache.lock().unwrap();
        let expired_keys: Vec<String> = cache.iter()
            .filter(|(_, entry)| entry.is_expired())
            .map(|(k, _)| k.clone())
            .collect();
        
        for key in expired_keys {
            cache.remove(&key);
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub sets: u64,
    pub deletes: u64,
    pub clears: u64,
}

impl CacheStats {
    pub fn total_requests(&self) -> u64 {
        self.hits + self.misses
    }

    pub fn hit_rate(&self) -> f64 {
        let total = self.total_requests();
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }

    pub fn miss_rate(&self) -> f64 {
        1.0 - self.hit_rate()
    }
}

/// 批量操作
pub struct BatchOperation<T: Clone> {
    operations: Vec<BatchItem<T>>,
    batch_size: usize,
}

#[derive(Debug, Clone)]
pub enum BatchItem<T> {
    Create(T),
    Update(String, T), // ID, data
    Delete(String),   // ID
}

impl<T: Clone> BatchOperation<T> {
    pub fn new(batch_size: usize) -> Self {
        Self {
            operations: Vec::new(),
            batch_size,
        }
    }

    pub fn add_create(mut self, item: T) -> Self {
        self.operations.push(BatchItem::Create(item));
        self
    }

    pub fn add_update(mut self, id: String, item: T) -> Self {
        self.operations.push(BatchItem::Update(id, item));
        self
    }

    pub fn add_delete(mut self, id: String) -> Self {
        self.operations.push(BatchItem::Delete(id));
        self
    }

    pub fn len(&self) -> usize {
        self.operations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.operations.len() >= self.batch_size
    }

    pub fn execute_in_batches<F, Fut>(&self, executor: F) -> impl Iterator<Item = Vec<BatchItem<T>>> + '_
    where
        F: Fn(Vec<BatchItem<T>>) -> Fut + Copy,
        Fut: std::future::Future<Output = ()>,
    {
        self.operations.chunks(self.batch_size)
            .map(|chunk| chunk.to_vec())
    }

    pub fn build_batch_sql(&self, table_name: &str) -> Vec<String> {
        let mut sqls = Vec::new();
        
        // Group operations by type for efficiency
        let mut creates = Vec::new();
        let mut updates = Vec::new();
        let mut deletes = Vec::new();

        for operation in &self.operations {
            match operation {
                BatchItem::Create(_) => creates.push(operation),
                BatchItem::Update(_, _) => updates.push(operation),
                BatchItem::Delete(_) => deletes.push(operation),
            }
        }

        // Build batch INSERT
        if !creates.is_empty() {
            // This would need to extract actual columns and values from the items
            // Simplified version
            sqls.push(format!(
                "INSERT INTO {} (column1, column2) VALUES (value1, value2), (value3, value4)",
                table_name
            ));
        }

        // Build batch UPDATE
        for update in &updates {
            if let BatchItem::Update(id, _) = update {
                sqls.push(format!(
                    "UPDATE {} SET column1 = value1 WHERE id = '{}'",
                    table_name, id
                ));
            }
        }

        // Build batch DELETE
        if !deletes.is_empty() {
            let ids: Vec<String> = deletes.iter()
                .filter_map(|item| {
                    if let BatchItem::Delete(id) = item {
                        Some(id.clone())
                    } else {
                        None
                    }
                })
                .collect();
            
            sqls.push(format!(
                "DELETE FROM {} WHERE id IN ('{}')",
                table_name,
                ids.join("', '")
            ));
        }

        sqls
    }
}

impl<T: Clone> Default for BatchOperation<T> {
    fn default() -> Self {
        Self::new(100)
    }
}

/// 预编译语句缓存（使用简化的 LRU）
pub struct PreparedStatementCache {
    cache: Arc<Mutex<SimpleLruCache<String, String>>>,
    capacity: usize,
}

impl PreparedStatementCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: Arc::new(Mutex::new(SimpleLruCache::new(capacity))),
            capacity,
        }
    }

    pub fn get(&self, key: &str) -> Option<String> {
        let mut cache = self.cache.lock().unwrap();
        cache.get(&key.to_string())
    }

    pub fn set(&self, key: &str, statement: String) {
        let mut cache = self.cache.lock().unwrap();
        cache.put(key.to_string(), statement);
    }

    pub fn clear(&self) {
        let mut cache = self.cache.lock().unwrap();
        cache.clear();
    }

    pub fn len(&self) -> usize {
        let cache = self.cache.lock().unwrap();
        cache.len()
    }
}

/// 连接池监控
pub struct ConnectionPoolMonitor {
    active_connections: Arc<Mutex<usize>>,
    idle_connections: Arc<Mutex<usize>>,
    total_connections: Arc<Mutex<usize>>,
    max_connections: usize,
    min_idle: usize,
}

impl ConnectionPoolMonitor {
    pub fn new(max_connections: usize, min_idle: usize) -> Self {
        Self {
            active_connections: Arc::new(Mutex::new(0)),
            idle_connections: Arc::new(Mutex::new(0)),
            total_connections: Arc::new(Mutex::new(0)),
            max_connections,
            min_idle,
        }
    }

    pub fn record_active(&self) {
        let mut active = self.active_connections.lock().unwrap();
        let mut idle = self.idle_connections.lock().unwrap();
        let mut total = self.total_connections.lock().unwrap();
        
        *active += 1;
        *idle = idle.saturating_sub(1);
        *total = total.max(*active + *idle);
    }

    pub fn record_idle(&self) {
        let mut active = self.active_connections.lock().unwrap();
        let mut idle = self.idle_connections.lock().unwrap();
        
        *active = active.saturating_sub(1);
        *idle += 1;
    }

    pub fn get_stats(&self) -> PoolStats {
        let active = *self.active_connections.lock().unwrap();
        let idle = *self.idle_connections.lock().unwrap();
        let total = *self.total_connections.lock().unwrap();

        PoolStats {
            active_connections: active,
            idle_connections: idle,
            total_connections: total,
            max_connections: self.max_connections,
            min_idle: self.min_idle,
            utilization_rate: if self.max_connections > 0 {
                active as f64 / self.max_connections as f64
            } else {
                0.0
            },
        }
    }

    pub fn needs_scaling_up(&self) -> bool {
        let stats = self.get_stats();
        stats.utilization_rate > 0.8 || stats.idle_connections < self.min_idle
    }

    pub fn needs_scaling_down(&self) -> bool {
        let stats = self.get_stats();
        stats.utilization_rate < 0.2 && stats.idle_connections > self.min_idle * 2
    }
}

#[derive(Debug, Clone)]
pub struct PoolStats {
    pub active_connections: usize,
    pub idle_connections: usize,
    pub total_connections: usize,
    pub max_connections: usize,
    pub min_idle: usize,
    pub utilization_rate: f64,
}

/// 性能优化配置
pub struct PerformanceConfig {
    pub query_cache_enabled: bool,
    pub query_cache_size: usize,
    pub query_cache_ttl: Option<Duration>,
    pub batch_size: usize,
    pub prepared_statement_cache_size: usize,
    pub connection_pool_max_size: usize,
    pub connection_pool_min_idle: usize,
    pub slow_query_threshold: Duration,
    pub enable_query_stats: bool,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            query_cache_enabled: true,
            query_cache_size: 1000,
            query_cache_ttl: Some(Duration::from_secs(3600)), // 1 hour
            batch_size: 100,
            prepared_statement_cache_size: 500,
            connection_pool_max_size: 10,
            connection_pool_min_idle: 2,
            slow_query_threshold: Duration::from_millis(1000),
            enable_query_stats: true,
        }
    }
}

/// 性能优化管理器
pub struct PerformanceManager {
    config: PerformanceConfig,
    query_cache_stats: Arc<Mutex<CacheStats>>,
    pool_monitor: ConnectionPoolMonitor,
}

impl PerformanceManager {
    pub fn new(config: PerformanceConfig) -> Self {
        Self {
            pool_monitor: ConnectionPoolMonitor::new(
                config.connection_pool_max_size,
                config.connection_pool_min_idle,
            ),
            query_cache_stats: Arc::new(Mutex::new(CacheStats::default())),
            config,
        }
    }

    pub fn get_config(&self) -> &PerformanceConfig {
        &self.config
    }

    pub fn update_config(&mut self, config: PerformanceConfig) {
        self.config = config;
    }

    pub fn get_pool_stats(&self) -> PoolStats {
        self.pool_monitor.get_stats()
    }

    pub fn get_query_cache_stats(&self) -> CacheStats {
        let stats = self.query_cache_stats.lock().unwrap();
        stats.clone()
    }

    pub fn should_scale_pool(&self) -> Option<PoolAction> {
        if self.pool_monitor.needs_scaling_up() {
            Some(PoolAction::ScaleUp)
        } else if self.pool_monitor.needs_scaling_down() {
            Some(PoolAction::ScaleDown)
        } else {
            None
        }
    }

    pub fn is_slow_query(&self, duration: Duration) -> bool {
        duration > self.config.slow_query_threshold
    }

    pub fn get_optimization_suggestions(&self) -> Vec<OptimizationSuggestion> {
        let mut suggestions = Vec::new();

        // Check pool utilization
        let pool_stats = self.get_pool_stats();
        if pool_stats.utilization_rate > 0.8 {
            suggestions.push(OptimizationSuggestion {
                suggestion_type: SuggestionType::PoolScaling,
                description: format!(
                    "Connection pool utilization is {:.1}%. Consider increasing max_connections.",
                    pool_stats.utilization_rate * 100.0
                ),
                priority: SuggestionPriority::High,
            });
        }

        // Check cache hit rate
        let cache_stats = self.get_query_cache_stats();
        if cache_stats.total_requests() > 100 && cache_stats.hit_rate() < 0.5 {
            suggestions.push(OptimizationSuggestion {
                suggestion_type: SuggestionType::CacheOptimization,
                description: format!(
                    "Query cache hit rate is {:.1}%. Consider adjusting cache TTL or size.",
                    cache_stats.hit_rate() * 100.0
                ),
                priority: SuggestionPriority::Medium,
            });
        }

        suggestions
    }
}

#[derive(Debug, Clone)]
pub enum PoolAction {
    ScaleUp,
    ScaleDown,
    NoAction,
}

#[derive(Debug, Clone)]
pub struct OptimizationSuggestion {
    pub suggestion_type: SuggestionType,
    pub description: String,
    pub priority: SuggestionPriority,
}

#[derive(Debug, Clone, Copy)]
pub enum SuggestionType {
    PoolScaling,
    CacheOptimization,
    QueryOptimization,
    IndexOptimization,
}

#[derive(Debug, Clone, Copy)]
pub enum SuggestionPriority {
    Low,
    Medium,
    High,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_entry_creation() {
        let entry = CacheEntry::new("test_data", Some(Duration::from_secs(60)));
        assert_eq!(entry.hit_count, 0);
        assert!(!entry.is_expired());
    }

    #[test]
    fn test_cache_entry_expiration() {
        let mut entry = CacheEntry::new("test_data", Some(Duration::from_millis(1)));
        std::thread::sleep(Duration::from_millis(10));
        assert!(entry.is_expired());
    }

    #[test]
    fn test_query_cache_basic() {
        let cache = QueryCache::new(100, Some(Duration::from_secs(60)));
        
        cache.set("key1", "value1");
        assert_eq!(cache.get("key1"), Some("value1"));
        assert_eq!(cache.get("key2"), None);
    }

    #[test]
    fn test_query_cache_with_ttl() {
        let cache = QueryCache::new(100, None);
        
        cache.set_with_ttl("key1", "value1", Duration::from_millis(1));
        std::thread::sleep(Duration::from_millis(10));
        assert_eq!(cache.get("key1"), None);
    }

    #[test]
    fn test_cache_stats() {
        let mut stats = CacheStats::default();
        stats.hits = 75;
        stats.misses = 25;
        
        assert_eq!(stats.total_requests(), 100);
        assert_eq!(stats.hit_rate(), 0.75);
        assert_eq!(stats.miss_rate(), 0.25);
    }

    #[test]
    fn test_batch_operation() {
        let batch = BatchOperation::new(10)
            .add_create("item1")
            .add_create("item2")
            .add_delete("id1".to_string());
        
        assert_eq!(batch.len(), 3);
        assert!(!batch.is_empty());
        assert!(!batch.is_full());
    }

    #[test]
    fn test_connection_pool_monitor() {
        let monitor = ConnectionPoolMonitor::new(10, 2);
        
        monitor.record_active();
        monitor.record_active();
        monitor.record_idle();
        
        let stats = monitor.get_stats();
        assert_eq!(stats.active_connections, 1);
        assert_eq!(stats.idle_connections, 1);
        assert_eq!(stats.total_connections, 2);
    }

    #[test]
    fn test_performance_config_default() {
        let config = PerformanceConfig::default();
        assert!(config.query_cache_enabled);
        assert_eq!(config.query_cache_size, 1000);
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.connection_pool_max_size, 10);
    }

    #[test]
    fn test_performance_manager() {
        let config = PerformanceConfig::default();
        let manager = PerformanceManager::new(config);
        
        let stats = manager.get_pool_stats();
        assert_eq!(stats.max_connections, 10);
        
        let cache_stats = manager.get_query_cache_stats();
        assert_eq!(cache_stats.total_requests(), 0);
    }

    #[test]
    fn test_pool_scaling_suggestions() {
        let monitor = ConnectionPoolMonitor::new(10, 2);
        
        // Simulate high utilization
        for _ in 0..9 {
            monitor.record_active();
        }
        
        assert!(monitor.needs_scaling_up());
        assert!(!monitor.needs_scaling_down());
    }

    #[test]
    fn test_cache_invalidation() {
        let cache = QueryCache::new(100, Some(Duration::from_secs(60)));
        
        cache.set("user:1", "data1");
        cache.set("user:2", "data2");
        cache.set("post:1", "data3");
        
        assert_eq!(cache.get("user:1"), Some("data1"));
        assert_eq!(cache.get("user:2"), Some("data2"));
        
        cache.invalidate_prefix("user:");
        
        assert_eq!(cache.get("user:1"), None);
        assert_eq!(cache.get("user:2"), None);
        assert_eq!(cache.get("post:1"), Some("data3"));
    }

    #[test]
    fn test_slow_query_detection() {
        let config = PerformanceConfig::default();
        let manager = PerformanceManager::new(config);
        
        assert!(manager.is_slow_query(Duration::from_millis(1500)));
        assert!(!manager.is_slow_query(Duration::from_millis(500)));
    }

    #[test]
    fn test_optimization_suggestions() {
        let config = PerformanceConfig::default();
        let manager = PerformanceManager::new(config);
        
        let suggestions = manager.get_optimization_suggestions();
        assert!(suggestions.is_empty()); // No suggestions under normal conditions
    }

    #[test]
    fn test_simple_lru_cache_integration() {
        let cache = QueryCache::new(3, None);
        
        cache.set("key1", "value1");
        cache.set("key2", "value2");
        cache.set("key3", "value3");
        cache.set("key4", "value4"); // Should evict key1
        
        assert_eq!(cache.get("key1"), None); // Evicted
        assert_eq!(cache.get("key2"), Some("value2"));
        assert_eq!(cache.get("key3"), Some("value3"));
        assert_eq!(cache.get("key4"), Some("value4"));
    }
}