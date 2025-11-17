use serde::{Deserialize, Serialize};
use thiserror::Error;

// ============================================================================
// ERROR TYPES
// ============================================================================

#[derive(Error, Debug)]
pub enum FluxError {
    #[error("Record not found")]
    NotFound,

    #[error("Model has no primary key value")]
    NoId,

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Transaction error: {0}")]
    Transaction(String),

    #[error("Migration error: {0}")]
    Migration(String),

    #[error("Validation failed: {0:?}")]
    Validation(Vec<ValidationError>),

    #[error("Query building error: {0}")]
    QueryBuild(String),

    #[error("Connection pool error: {0}")]
    Pool(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

pub type Result<T> = std::result::Result<T, FluxError>;
