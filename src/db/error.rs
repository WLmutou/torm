use thiserror::Error;

pub type Result<T> = std::result::Result<T, TormError>;

#[derive(Error, Debug)]
pub enum TormError {
    #[error("Database connection error: {0}")]
    ConnectionError(String),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Model not found")]
    NotFound,

    #[error("Invalid query: {0}")]
    InvalidQuery(String),

    #[error("Transaction error: {0}")]
    TransactionError(String),

    #[error("Migration error: {0}")]
    MigrationError(String),

    #[error("Hook error: {0}")]
    HookError(String),

    #[error("Unimplemented feature: {0}")]
    Unimplemented(String),

    #[error("Custom error: {0}")]
    Custom(String),

    #[error("Database error: {0}")]
    DatabaseError(String),
}

impl TormError {
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

    pub fn database_error(msg: impl Into<String>) -> Self {
        Self::DatabaseError(msg.into())
    }
}

// Convert from our custom DbError to TormError
impl From<crate::db::database::DbError> for TormError {
    fn from(err: crate::db::database::DbError) -> Self {
        match err {
            crate::db::database::DbError::ConnectionError(msg) => TormError::ConnectionError(msg),
            crate::db::database::DbError::QueryError(msg) => TormError::InvalidQuery(msg),
            crate::db::database::DbError::ExecutionError(msg) => TormError::DatabaseError(msg),
            crate::db::database::DbError::TransactionError(msg) => TormError::TransactionError(msg),
            crate::db::database::DbError::ParseError(msg) => TormError::InvalidQuery(msg),
            crate::db::database::DbError::TimeoutError(msg) => TormError::ConnectionError(msg),
            crate::db::database::DbError::IoError(msg) => TormError::ConnectionError(msg),
            crate::db::database::DbError::ProtocolError(msg) => TormError::ConnectionError(msg),
            crate::db::database::DbError::AuthError(msg) => TormError::ConnectionError(msg),
            crate::db::database::DbError::ConstraintError(msg) => TormError::InvalidQuery(msg),
            crate::db::database::DbError::PoolError(msg) => TormError::ConnectionError(msg),
            crate::db::database::DbError::NotFound => TormError::NotFound,
        }
    }
}

// Convert from TormError to DbError (used by the GORM-style model persistence API)
impl From<TormError> for crate::db::database::DbError {
    fn from(err: TormError) -> Self {
        match err {
            TormError::ConnectionError(msg) => crate::db::database::DbError::connection_error(msg),
            TormError::SerializationError(msg) => {
                crate::db::database::DbError::ParseError(msg.to_string())
            }
            TormError::NotFound => crate::db::database::DbError::NotFound,
            TormError::InvalidQuery(msg) => crate::db::database::DbError::query_error(msg),
            TormError::TransactionError(msg) => {
                crate::db::database::DbError::transaction_error(msg)
            }
            TormError::MigrationError(msg) => crate::db::database::DbError::execution_error(msg),
            TormError::HookError(msg) => crate::db::database::DbError::execution_error(msg),
            TormError::Unimplemented(msg) => crate::db::database::DbError::protocol_error(msg),
            TormError::Custom(msg) => crate::db::database::DbError::execution_error(msg),
            TormError::DatabaseError(msg) => crate::db::database::DbError::execution_error(msg),
        }
    }
}
