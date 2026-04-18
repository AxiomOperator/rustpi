use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("session not found: {0}")]
    SessionNotFound(agent_core::types::SessionId),
    #[error("run not found: {0}")]
    RunNotFound(agent_core::types::RunId),
    #[error("database error: {0}")]
    Database(String),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("migration failed: {0}")]
    Migration(String),
}
