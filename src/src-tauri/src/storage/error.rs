use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum StorageError {
    DatabaseError(String),
    NotFound(String),
    InvalidInput(String),
    SerializationError(String),
    MigrationError(String),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageError::DatabaseError(msg) => write!(f, "Database error: {}", msg),
            StorageError::NotFound(msg) => write!(f, "Not found: {}", msg),
            StorageError::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
            StorageError::SerializationError(msg) => write!(f, "Serialization error: {}", msg),
            StorageError::MigrationError(msg) => write!(f, "Migration error: {}", msg),
        }
    }
}

impl std::error::Error for StorageError {}

impl From<sqlx::Error> for StorageError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => StorageError::NotFound("Record not found".to_string()),
            _ => StorageError::DatabaseError(err.to_string()),
        }
    }
}

impl From<serde_json::Error> for StorageError {
    fn from(err: serde_json::Error) -> Self {
        StorageError::SerializationError(err.to_string())
    }
}

impl From<std::io::Error> for StorageError {
    fn from(err: std::io::Error) -> Self {
        StorageError::DatabaseError(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, StorageError>;
