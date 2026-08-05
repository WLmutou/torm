use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// 简单连接池
pub struct SimplePool<T> {
    connections: Arc<Mutex<VecDeque<T>>>,
    max_size: usize,
    min_idle: usize,
    timeout: Duration,
    created_connections: Arc<Mutex<usize>>,
}

impl<T> SimplePool<T> {
    pub fn new(connections: Vec<T>) -> Self {
        let max_size = connections.len();
        let connections_arc = Arc::new(Mutex::new(VecDeque::from_iter(connections)));
        
        Self {
            connections: connections_arc,
            max_size,
            min_idle: 1.min(max_size),
            timeout: Duration::from_secs(30),
            created_connections: Arc::new(Mutex::new(max_size)),
        }
    }

    pub async fn get(&self) -> Result<T, String> {
        let start = Instant::now();
        
        while start.elapsed() < self.timeout {
            // Try to get a connection
            {
                let mut conns = self.connections.lock().unwrap();
                if let Some(conn) = conns.pop_front() {
                    return Ok(conn);
                }
            }
            
            // Wait a bit before retrying
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        
        Err("Connection timeout".to_string())
    }

    pub fn put(&self, conn: T) {
        let mut conns = self.connections.lock().unwrap();
        if conns.len() < self.max_size {
            conns.push_back(conn);
        }
    }

    pub fn size(&self) -> usize {
        *self.created_connections.lock().unwrap()
    }

    pub fn status(&self) -> PoolStatus {
        let conns = self.connections.lock().unwrap();
        PoolStatus {
            total_connections: self.max_size,
            idle_connections: conns.len(),
            active_connections: self.max_size - conns.len(),
        }
    }

    pub fn close(&self) {
        let mut conns = self.connections.lock().unwrap();
        conns.clear();
    }
}

/// 连接池状态
#[derive(Debug, Clone)]
pub struct PoolStatus {
    pub total_connections: usize,
    pub idle_connections: usize,
    pub active_connections: usize,
}

impl PoolStatus {
    pub fn utilization_rate(&self) -> f64 {
        if self.total_connections == 0 {
            0.0
        } else {
            self.active_connections as f64 / self.total_connections as f64
        }
    }
}

/// 数据库连接池包装器 - 使用我们的自定义连接类型
pub enum DbPool {
    MySQL(SimplePool<crate::db::mysql::MySqlConnection>),
    PostgreSQL(SimplePool<crate::db::postgresql::PostgresConnection>),
    SQLite(SimplePool<crate::db::sqlite::SqliteConnection>),
}

impl DbPool {
    pub async fn new(driver: crate::db::driver::DBDriver, pool_size: usize) -> Result<Self, String> {
        let pool = match driver {
            crate::db::driver::DBDriver::MySQL => {
                let mut connections = Vec::new();
                let config = crate::db::database::ConnectionConfig::mysql("localhost", 3306, "test", "user", "pass");
                for _ in 0..pool_size {
                    let conn = crate::db::mysql::MySqlConnection::new(&config).await
                        .map_err(|e| format!("MySQL connection failed: {}", e))?;
                    connections.push(conn);
                }
                DbPool::MySQL(SimplePool::new(connections))
            }
            crate::db::driver::DBDriver::PostgreSQL => {
                let mut connections = Vec::new();
                let config = crate::db::database::ConnectionConfig::postgresql("localhost", 5432, "test", "user", "pass");
                for _ in 0..pool_size {
                    let conn = crate::db::postgresql::PostgresConnection::new(&config).await
                        .map_err(|e| format!("PostgreSQL connection failed: {}", e))?;
                    connections.push(conn);
                }
                DbPool::PostgreSQL(SimplePool::new(connections))
            }
            crate::db::driver::DBDriver::SQLite => {
                let mut connections = Vec::new();
                for _ in 0..pool_size {
                    let conn = crate::db::sqlite::SqliteConnection::new(":memory:").await
                        .map_err(|e| format!("SQLite connection failed: {}", e))?;
                    connections.push(conn);
                }
                DbPool::SQLite(SimplePool::new(connections))
            }
        };
        Ok(pool)
    }

    pub fn driver(&self) -> crate::db::driver::DBDriver {
        match self {
            DbPool::MySQL(_) => crate::db::driver::DBDriver::MySQL,
            DbPool::PostgreSQL(_) => crate::db::driver::DBDriver::PostgreSQL,
            DbPool::SQLite(_) => crate::db::driver::DBDriver::SQLite,
        }
    }

    pub fn status(&self) -> PoolStatus {
        match self {
            DbPool::MySQL(pool) => pool.status(),
            DbPool::PostgreSQL(pool) => pool.status(),
            DbPool::SQLite(pool) => pool.status(),
        }
    }

    pub fn size(&self) -> usize {
        match self {
            DbPool::MySQL(pool) => pool.size(),
            DbPool::PostgreSQL(pool) => pool.size(),
            DbPool::SQLite(pool) => pool.size(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_pool() {
        let pool = SimplePool::new(vec![1, 2, 3]);
        assert_eq!(pool.size(), 3);
        
        let status = pool.status();
        assert_eq!(status.total_connections, 3);
        assert_eq!(status.idle_connections, 3);
        assert_eq!(status.active_connections, 0);
    }

    #[tokio::test]
    async fn test_pool_get_put() {
        let pool = SimplePool::new(vec![1, 2, 3]);
        
        let conn = pool.get().await.unwrap();
        assert_eq!(conn, 1);
        
        let status = pool.status();
        assert_eq!(status.idle_connections, 2);
        assert_eq!(status.active_connections, 1);
        
        pool.put(conn);
        
        let status = pool.status();
        assert_eq!(status.idle_connections, 3);
        assert_eq!(status.active_connections, 0);
    }
}
