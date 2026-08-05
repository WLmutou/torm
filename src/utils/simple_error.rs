use std::fmt;

/// 简化的错误类型，不依赖 thiserror
#[derive(Debug, Clone)]
pub enum SimpleError {
    ConnectionError(String),
    PoolError(String),
    SerializationError(String),
    NotFound,
    InvalidQuery(String),
    TransactionError(String),
    MigrationError(String),
    HookError(String),
    Custom(String),
}

impl fmt::Display for SimpleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SimpleError::ConnectionError(msg) => write!(f, "Connection error: {}", msg),
            SimpleError::PoolError(msg) => write!(f, "Pool error: {}", msg),
            SimpleError::SerializationError(msg) => write!(f, "Serialization error: {}", msg),
            SimpleError::NotFound => write!(f, "Record not found"),
            SimpleError::InvalidQuery(msg) => write!(f, "Invalid query: {}", msg),
            SimpleError::TransactionError(msg) => write!(f, "Transaction error: {}", msg),
            SimpleError::MigrationError(msg) => write!(f, "Migration error: {}", msg),
            SimpleError::HookError(msg) => write!(f, "Hook error: {}", msg),
            SimpleError::Custom(msg) => write!(f, "Error: {}", msg),
        }
    }
}

impl std::error::Error for SimpleError {}

impl From<crate::db::database::DbError> for SimpleError {
    fn from(err: crate::db::database::DbError) -> Self {
        SimpleError::ConnectionError(err.to_string())
    }
}

impl From<crate::db::storage::StorageError> for SimpleError {
    fn from(err: crate::db::storage::StorageError) -> Self {
        SimpleError::Custom(err.to_string())
    }
}

impl From<serde_json::Error> for SimpleError {
    fn from(err: serde_json::Error) -> Self {
        SimpleError::SerializationError(format!("{}", err))
    }
}

impl SimpleError {
    pub fn custom(msg: impl Into<String>) -> Self {
        Self::Custom(msg.into())
    }

    pub fn invalid_query(msg: impl Into<String>) -> Self {
        Self::InvalidQuery(msg.into())
    }

    pub fn transaction_error(msg: impl Into<String>) -> Self {
        Self::TransactionError(msg.into())
    }

    pub fn migration_error(msg: impl Into<String>) -> Self {
        Self::MigrationError(msg.into())
    }

    pub fn hook_error(msg: impl Into<String>) -> Self {
        Self::HookError(msg.into())
    }

    pub fn connection_error(msg: impl Into<String>) -> Self {
        Self::ConnectionError(msg.into())
    }

    pub fn pool_error(msg: impl Into<String>) -> Self {
        Self::PoolError(msg.into())
    }
}

/// 简化的 Result 类型
pub type SimpleResult<T> = Result<T, SimpleError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let error = SimpleError::custom("Custom error message");
        assert!(error.to_string().contains("Custom error message"));
    }

    #[test]
    fn test_error_display() {
        let error = SimpleError::NotFound;
        assert_eq!(error.to_string(), "Record not found");
    }

    #[test]
    fn test_error_types() {
        let errors = vec![
            SimpleError::invalid_query("Invalid SQL"),
            SimpleError::transaction_error("Transaction failed"),
            SimpleError::migration_error("Migration error"),
            SimpleError::hook_error("Before hook failed"),
        ];

        for error in errors {
            assert!(!error.to_string().is_empty());
        }
    }

    #[test]
    fn test_error_from_db_error() {
        let db_error = crate::db::database::DbError::NotFound;
        let simple_error: SimpleError = db_error.into();
        assert!(matches!(simple_error, SimpleError::ConnectionError(_)));
    }

    #[test]
    fn test_simple_result() {
        let success: SimpleResult<i32> = Ok(42);
        assert_eq!(success.unwrap(), 42);

        let failure: SimpleResult<i32> = Err(SimpleError::NotFound);
        assert!(failure.is_err());
    }
}
