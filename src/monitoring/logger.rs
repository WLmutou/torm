use std::sync::Arc;
use std::time::Instant;
use chrono::Utc;
use std::io::Write;

/// 日志级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Debug = 0,
    Info = 1,
    Warn = 2,
    Error = 3,
}

impl LogLevel {
    pub fn as_str(&self) -> &str {
        match self {
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        }
    }

    pub fn as_color_code(&self) -> &str {
        match self {
            LogLevel::Debug => "\x1b[36m",    // Cyan
            LogLevel::Info => "\x1b[32m",     // Green
            LogLevel::Warn => "\x1b[33m",     // Yellow
            LogLevel::Error => "\x1b[31m",    // Red
        }
    }
}

/// 日志记录
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub level: LogLevel,
    pub message: String,
    pub timestamp: chrono::DateTime<Utc>,
    pub context: Option<String>,
    pub query_duration: Option<std::time::Duration>,
    pub query_sql: Option<String>,
    pub error: Option<String>,
}

impl LogEntry {
    pub fn new(level: LogLevel, message: &str) -> Self {
        Self {
            level,
            message: message.to_string(),
            timestamp: Utc::now(),
            context: None,
            query_duration: None,
            query_sql: None,
            error: None,
        }
    }

    pub fn with_context(mut self, context: &str) -> Self {
        self.context = Some(context.to_string());
        self
    }

    pub fn with_query(mut self, sql: &str, duration: std::time::Duration) -> Self {
        self.query_sql = Some(sql.to_string());
        self.query_duration = Some(duration);
        self
    }

    pub fn with_error(mut self, error: &str) -> Self {
        self.error = Some(error.to_string());
        self
    }

    pub fn format(&self) -> String {
        let mut output = String::new();
        
        // Timestamp
        output.push_str(&format!("[{}] ", self.timestamp.format("%Y-%m-%d %H:%M:%S%.3f")));
        
        // Level with color
        output.push_str(&format!("{}{}{}\x1b[0m ", 
            self.level.as_color_code(), 
            self.level.as_str(), 
            "\x1b[0m"));
        
        // Message
        output.push_str(&self.message);

        // Query info
        if let (Some(sql), Some(duration)) = (&self.query_sql, &self.query_duration) {
            output.push_str(&format!(" | Query: {} | Duration: {:?}", sql, duration));
            
            // Slow query warning
            if duration.as_millis() > 1000 {
                output.push_str(" \x1b[31m[SLOW QUERY]\x1b[0m");
            }
        }

        // Context
        if let Some(context) = &self.context {
            output.push_str(&format!(" | Context: {}", context));
        }

        // Error
        if let Some(error) = &self.error {
            output.push_str(&format!(" | Error: {}", error));
        }

        output
    }
}

/// Logger trait
pub trait Logger: Send + Sync {
    fn log(&self, entry: &LogEntry);
    fn set_level(&mut self, level: LogLevel);
    fn get_level(&self) -> LogLevel;
}

/// 控制台 Logger
pub struct ConsoleLogger {
    level: LogLevel,
}

impl ConsoleLogger {
    pub fn new(level: LogLevel) -> Self {
        Self { level }
    }
}

impl Logger for ConsoleLogger {
    fn log(&self, entry: &LogEntry) {
        if entry.level >= self.level {
            println!("{}", entry.format());
        }
    }

    fn set_level(&mut self, level: LogLevel) {
        self.level = level;
    }

    fn get_level(&self) -> LogLevel {
        self.level
    }
}

/// 文件 Logger
pub struct FileLogger {
    level: LogLevel,
    file: Option<std::fs::File>,
    path: String,
}

impl FileLogger {
    pub fn new(level: LogLevel, path: &str) -> Result<Self, std::io::Error> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;

        Ok(Self {
            level,
            file: Some(file),
            path: path.to_string(),
        })
    }

    pub fn rotate(&mut self) -> Result<(), std::io::Error> {
        // Simple rotation - move current file and create new one
        if let Some(file) = &self.file {
            drop(file);
        }

        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let rotated_path = format!("{}.{}", self.path, timestamp);
        std::fs::rename(&self.path, &rotated_path)?;

        let new_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;

        self.file = Some(new_file);
        Ok(())
    }
}

impl Logger for FileLogger {
    fn log(&self, entry: &LogEntry) {
        if entry.level >= self.level {
            let formatted = entry.format();
            if let Some(file) = &self.file {
                let mut file = file;
                let _ = writeln!(file, "{}", formatted);
                let _ = file.flush();
            }
        }
    }

    fn set_level(&mut self, level: LogLevel) {
        self.level = level;
    }

    fn get_level(&self) -> LogLevel {
        self.level
    }
}

/// 组合 Logger
pub struct CompositeLogger {
    loggers: Vec<Box<dyn Logger>>,
}

impl CompositeLogger {
    pub fn new() -> Self {
        Self {
            loggers: Vec::new(),
        }
    }

    pub fn add_logger(mut self, logger: Box<dyn Logger>) -> Self {
        self.loggers.push(logger);
        self
    }
}

impl Logger for CompositeLogger {
    fn log(&self, entry: &LogEntry) {
        for logger in &self.loggers {
            logger.log(entry);
        }
    }

    fn set_level(&mut self, level: LogLevel) {
        for logger in &mut self.loggers {
            logger.set_level(level);
        }
    }

    fn get_level(&self) -> LogLevel {
        // Return the minimum level among all loggers
        self.loggers.iter()
            .map(|l| l.get_level())
            .min()
            .unwrap_or(LogLevel::Info)
    }
}

/// 查询执行追踪器
pub struct QueryTracer {
    logger: Arc<dyn Logger>,
    slow_query_threshold: std::time::Duration,
    start_time: Instant,
    sql: String,
}

impl QueryTracer {
    pub fn new(logger: Arc<dyn Logger>, sql: &str) -> Self {
        Self {
            logger,
            slow_query_threshold: std::time::Duration::from_millis(1000),
            start_time: Instant::now(),
            sql: sql.to_string(),
        }
    }

    pub fn with_slow_threshold(mut self, threshold: std::time::Duration) -> Self {
        self.slow_query_threshold = threshold;
        self
    }

    pub fn finish(self) {
        let duration = self.start_time.elapsed();
        
        let level = if duration > self.slow_query_threshold {
            LogLevel::Warn
        } else {
            LogLevel::Debug
        };

        let entry = LogEntry::new(level, "Query executed")
            .with_query(&self.sql, duration);

        self.logger.log(&entry);
    }

    pub fn finish_with_error(self, error: &str) {
        let duration = self.start_time.elapsed();
        
        let entry = LogEntry::new(LogLevel::Error, "Query failed")
            .with_query(&self.sql, duration)
            .with_error(error);

        self.logger.log(&entry);
    }
}

/// 全局日志管理器
pub struct LogManager {
    logger: Arc<dyn Logger>,
}

impl LogManager {
    pub fn new(logger: Box<dyn Logger>) -> Self {
        Self {
            logger: Arc::from(logger),
        }
    }

    pub fn get_logger(&self) -> Arc<dyn Logger> {
        Arc::clone(&self.logger)
    }

    pub fn create_tracer(&self, sql: &str) -> QueryTracer {
        QueryTracer::new(Arc::clone(&self.logger), sql)
    }

    pub fn log(&self, level: LogLevel, message: &str) {
        let entry = LogEntry::new(level, message);
        self.logger.log(&entry);
    }

    pub fn log_with_context(&self, level: LogLevel, message: &str, context: &str) {
        let entry = LogEntry::new(level, message).with_context(context);
        self.logger.log(&entry);
    }

    pub fn debug(&self, message: &str) {
        self.log(LogLevel::Debug, message);
    }

    pub fn info(&self, message: &str) {
        self.log(LogLevel::Info, message);
    }

    pub fn warn(&self, message: &str) {
        self.log(LogLevel::Warn, message);
    }

    pub fn error(&self, message: &str) {
        self.log(LogLevel::Error, message);
    }
}

impl Clone for LogManager {
    fn clone(&self) -> Self {
        Self {
            logger: Arc::clone(&self.logger),
        }
    }
}

/// 性能监控器
pub struct PerformanceMonitor {
    query_times: Vec<std::time::Duration>,
    error_count: usize,
    slow_query_count: usize,
    slow_query_threshold: std::time::Duration,
}

impl PerformanceMonitor {
    pub fn new(slow_query_threshold: std::time::Duration) -> Self {
        Self {
            query_times: Vec::new(),
            error_count: 0,
            slow_query_count: 0,
            slow_query_threshold,
        }
    }

    pub fn record_query(&mut self, duration: std::time::Duration, success: bool) {
        self.query_times.push(duration);
        
        if !success {
            self.error_count += 1;
        }
        
        if duration > self.slow_query_threshold {
            self.slow_query_count += 1;
        }

        // Keep only last 1000 queries
        if self.query_times.len() > 1000 {
            self.query_times.remove(0);
        }
    }

    pub fn get_stats(&self) -> PerformanceStats {
        if self.query_times.is_empty() {
            return PerformanceStats {
                total_queries: 0,
                average_duration: std::time::Duration::ZERO,
                min_duration: std::time::Duration::ZERO,
                max_duration: std::time::Duration::ZERO,
                error_count: 0,
                slow_query_count: 0,
                error_rate: 0.0,
                slow_query_rate: 0.0,
            };
        }

        let total_duration: std::time::Duration = self.query_times.iter().sum();
        let avg_duration = total_duration / self.query_times.len() as u32;
        let min_duration = *self.query_times.iter().min().unwrap();
        let max_duration = *self.query_times.iter().max().unwrap();
        let error_rate = self.error_count as f64 / self.query_times.len() as f64;
        let slow_query_rate = self.slow_query_count as f64 / self.query_times.len() as f64;

        PerformanceStats {
            total_queries: self.query_times.len(),
            average_duration: avg_duration,
            min_duration,
            max_duration,
            error_count: self.error_count,
            slow_query_count: self.slow_query_count,
            error_rate,
            slow_query_rate,
        }
    }

    pub fn reset(&mut self) {
        self.query_times.clear();
        self.error_count = 0;
        self.slow_query_count = 0;
    }
}

#[derive(Debug)]
pub struct PerformanceStats {
    pub total_queries: usize,
    pub average_duration: std::time::Duration,
    pub min_duration: std::time::Duration,
    pub max_duration: std::time::Duration,
    pub error_count: usize,
    pub slow_query_count: usize,
    pub error_rate: f64,
    pub slow_query_rate: f64,
}

impl std::fmt::Display for PerformanceStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Performance Stats:\n")?;
        write!(f, "  Total Queries: {}\n", self.total_queries)?;
        write!(f, "  Average Duration: {:?}\n", self.average_duration)?;
        write!(f, "  Min Duration: {:?}\n", self.min_duration)?;
        write!(f, "  Max Duration: {:?}\n", self.max_duration)?;
        write!(f, "  Error Count: {} ({:.2}%)\n", self.error_count, self.error_rate * 100.0)?;
        write!(f, "  Slow Query Count: {} ({:.2}%)", self.slow_query_count, self.slow_query_rate * 100.0)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_entry_formatting() {
        let entry = LogEntry::new(LogLevel::Info, "Test message");
        let formatted = entry.format();
        assert!(formatted.contains("INFO"));
        assert!(formatted.contains("Test message"));
    }

    #[test]
    fn test_log_entry_with_query() {
        let entry = LogEntry::new(LogLevel::Debug, "Query executed")
            .with_query("SELECT * FROM users", std::time::Duration::from_millis(50));
        
        let formatted = entry.format();
        assert!(formatted.contains("SELECT * FROM users"));
        assert!(formatted.contains("Duration:"));
    }

    #[test]
    fn test_log_entry_with_error() {
        let entry = LogEntry::new(LogLevel::Error, "Query failed")
            .with_query("SELECT * FROM users", std::time::Duration::from_millis(100))
            .with_error("Connection timeout");
        
        let formatted = entry.format();
        assert!(formatted.contains("ERROR"));
        assert!(formatted.contains("Connection timeout"));
    }

    #[test]
    fn test_log_levels() {
        assert_eq!(LogLevel::Debug.as_str(), "DEBUG");
        assert_eq!(LogLevel::Info.as_str(), "INFO");
        assert_eq!(LogLevel::Warn.as_str(), "WARN");
        assert_eq!(LogLevel::Error.as_str(), "ERROR");
    }

    #[test]
    fn test_log_level_comparison() {
        assert!(LogLevel::Error > LogLevel::Warn);
        assert!(LogLevel::Warn > LogLevel::Info);
        assert!(LogLevel::Info > LogLevel::Debug);
    }

    #[test]
    fn test_console_logger_level_filtering() {
        let mut logger = ConsoleLogger::new(LogLevel::Info);
        
        let debug_entry = LogEntry::new(LogLevel::Debug, "Debug message");
        let info_entry = LogEntry::new(LogLevel::Info, "Info message");
        let error_entry = LogEntry::new(LogLevel::Error, "Error message");

        logger.set_level(LogLevel::Warn);
        
        // Should not log debug and info
        assert!(debug_entry.level < logger.get_level());
        assert!(info_entry.level < logger.get_level());
        assert!(error_entry.level >= logger.get_level());
    }

    #[test]
    fn test_query_tracer_timing() {
        use std::thread;
        
        let logger = ConsoleLogger::new(LogLevel::Debug);
        let tracer = QueryTracer::new(Arc::new(logger), "SELECT 1");
        
        thread::sleep(std::time::Duration::from_millis(10));
        
        // This would normally log the query execution time
        let _ = tracer;
    }

    #[test]
    fn test_performance_monitor() {
        let mut monitor = PerformanceMonitor::new(std::time::Duration::from_millis(100));
        
        monitor.record_query(std::time::Duration::from_millis(50), true);
        monitor.record_query(std::time::Duration::from_millis(150), true);
        monitor.record_query(std::time::Duration::from_millis(75), false);
        
        let stats = monitor.get_stats();
        assert_eq!(stats.total_queries, 3);
        assert_eq!(stats.error_count, 1);
        assert_eq!(stats.slow_query_count, 1);
        assert!(stats.average_duration.as_millis() > 0);
    }

    #[test]
    fn test_performance_monitor_stats_display() {
        let mut monitor = PerformanceMonitor::new(std::time::Duration::from_millis(100));
        
        monitor.record_query(std::time::Duration::from_millis(50), true);
        
        let stats = monitor.get_stats();
        let display = format!("{}", stats);
        assert!(display.contains("Performance Stats"));
        assert!(display.contains("Total Queries: 1"));
    }

    #[test]
    fn test_log_manager_convenience_methods() {
        let logger = Box::new(ConsoleLogger::new(LogLevel::Info));
        let manager = LogManager::new(logger);
        
        // These should not panic
        manager.debug("Debug message");
        manager.info("Info message");
        manager.warn("Warning message");
        manager.error("Error message");
    }

    #[test]
    fn test_composite_logger() {
        let composite = CompositeLogger::new()
            .add_logger(Box::new(ConsoleLogger::new(LogLevel::Info)));
        
        let entry = LogEntry::new(LogLevel::Info, "Test");
        composite.log(&entry);
    }

    #[test]
    fn test_performance_monitor_reset() {
        let mut monitor = PerformanceMonitor::new(std::time::Duration::from_millis(100));
        
        monitor.record_query(std::time::Duration::from_millis(50), true);
        monitor.record_query(std::time::Duration::from_millis(75), true);
        
        assert_eq!(monitor.get_stats().total_queries, 2);
        
        monitor.reset();
        assert_eq!(monitor.get_stats().total_queries, 0);
    }
}