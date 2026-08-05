use serde::{Deserialize, Serialize};
use std::fmt::{self, Display};

/// 数据库驱动类型（重新映射到新的 DbType）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DBDriver {
    MySQL,
    PostgreSQL,
    SQLite,
}

impl Display for DBDriver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DBDriver::MySQL => write!(f, "MySQL"),
            DBDriver::PostgreSQL => write!(f, "PostgreSQL"),
            DBDriver::SQLite => write!(f, "SQLite"),
        }
    }
}

impl DBDriver {
    pub fn to_db_type(&self) -> crate::db::db_types::DbType {
        match self {
            DBDriver::MySQL => crate::db::db_types::DbType::MySQL,
            DBDriver::PostgreSQL => crate::db::db_types::DbType::PostgreSQL,
            DBDriver::SQLite => crate::db::db_types::DbType::SQLite,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dsn {
    pub driver: DBDriver,
    pub database: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub options: String,
}

impl Dsn {
    pub fn new(driver: DBDriver, database: &str) -> Self {
        Self {
            driver,
            database: database.to_string(),
            host: match driver {
                DBDriver::MySQL => "localhost".to_string(),
                DBDriver::PostgreSQL => "localhost".to_string(),
                DBDriver::SQLite => String::new(),
            },
            port: match driver {
                DBDriver::MySQL => 3306,
                DBDriver::PostgreSQL => 5432,
                DBDriver::SQLite => 0,
            },
            username: String::new(),
            password: String::new(),
            options: String::new(),
        }
    }

    pub fn to_connection_config(&self) -> crate::db::database::ConnectionConfig {
        match self.driver {
            DBDriver::MySQL => crate::db::database::ConnectionConfig::mysql(
                &self.host,
                self.port,
                &self.database,
                &self.username,
                &self.password,
            ),
            DBDriver::PostgreSQL => crate::db::database::ConnectionConfig::postgresql(
                &self.host,
                self.port,
                &self.database,
                &self.username,
                &self.password,
            ),
            DBDriver::SQLite => crate::db::database::ConnectionConfig::sqlite(&self.database),
        }
    }

    pub fn with_host(mut self, host: &str) -> Self {
        self.host = host.to_string();
        self
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub fn with_username(mut self, username: &str) -> Self {
        self.username = username.to_string();
        self
    }

    pub fn with_password(mut self, password: &str) -> Self {
        self.password = password.to_string();
        self
    }

    pub fn with_options(mut self, options: &str) -> Self {
        self.options = options.to_string();
        self
    }

    pub fn build(&self) -> String {
        match self.driver {
            DBDriver::MySQL => {
                format!(
                    "mysql://{}:{}@{}:{}/{}{}",
                    self.username,
                    self.password,
                    self.host,
                    self.port,
                    self.database,
                    if self.options.is_empty() {
                        String::new()
                    } else {
                        format!("?{}", self.options)
                    }
                )
            }
            DBDriver::PostgreSQL => {
                format!(
                    "postgresql://{}:{}@{}:{}/{}{}",
                    self.username,
                    self.password,
                    self.host,
                    self.port,
                    self.database,
                    if self.options.is_empty() {
                        String::new()
                    } else {
                        format!("?{}", self.options)
                    }
                )
            }
            DBDriver::SQLite => {
                format!(
                    "{}{}",
                    self.database,
                    if self.options.is_empty() {
                        String::new()
                    } else {
                        format!("?{}", self.options)
                    }
                )
            }
        }
    }
}

impl Default for Dsn {
    fn default() -> Self {
        Self::new(DBDriver::SQLite, "torm.db")
    }
}
