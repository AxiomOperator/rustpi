use thiserror::Error;

#[derive(Debug, Error)]
pub enum EventLogError {
    #[error("write failed: {0}")]
    WriteFailed(String),
    #[error("read failed: {0}")]
    ReadFailed(String),
    #[error("corrupt event at offset {offset}: {reason}")]
    CorruptEvent { offset: u64, reason: String },
    #[error(transparent)]
    Serialization(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
