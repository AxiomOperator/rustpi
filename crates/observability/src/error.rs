use thiserror::Error;

#[derive(Debug, Error)]
pub enum ObservabilityError {
    #[error("lock poisoned: {0}")]
    LockPoisoned(String),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
