use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("session not found: {0}")]
    SessionNotFound(agent_core::types::SessionId),
    #[error("run not found: {0}")]
    RunNotFound(agent_core::types::RunId),
    #[error("summary not found: {0}")]
    SummaryNotFound(String),
    #[error("memory not found: {0}")]
    MemoryNotFound(String),
    #[error("database error: {0}")]
    Database(String),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("migration failed: {0}")]
    Migration(String),
    #[error("connection error: {0}")]
    Connection(String),
    #[error("version mismatch: found {found}, expected {expected}")]
    VersionMismatch { found: u32, expected: u32 },
    #[error("io error: {0}")]
    Io(String),
}

impl From<sqlx::Error> for StoreError {
    fn from(e: sqlx::Error) -> Self {
        StoreError::Database(e.to_string())
    }
}

impl From<sled::Error> for StoreError {
    fn from(e: sled::Error) -> Self {
        StoreError::Database(e.to_string())
    }
}
