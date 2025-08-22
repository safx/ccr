use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CcrError {
    // IO-related errors
    #[error("Failed to read file: {path}")]
    FileRead {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to access directory: {path}")]
    DirectoryAccess {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to read from stdin")]
    StdinRead(#[from] std::io::Error),

    // Data processing errors
    #[error("Failed to parse JSON: {context}")]
    JsonParse {
        context: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("Failed to parse JSON from stdin")]
    StdinJsonParse(#[from] serde_json::Error),

    #[error("Invalid data format: {message}")]
    DataValidation { message: String },

    // Environment-related errors
    #[error("Claude data directory not found")]
    ClaudePathNotFound,

    #[error("Environment variable '{var}' not set")]
    EnvVarMissing { var: String },

    #[error("Environment variable error")]
    EnvVar(#[from] std::env::VarError),

    // Async processing
    #[error("Task failed")]
    TaskJoin(#[from] tokio::task::JoinError),
}

pub type Result<T> = std::result::Result<T, CcrError>;
