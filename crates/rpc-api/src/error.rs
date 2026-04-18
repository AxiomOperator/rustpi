use thiserror::Error;

#[derive(Debug, Error)]
pub enum RpcError {
    #[error("protocol framing error: {0}")]
    Framing(String),
    #[error("unknown method: {0}")]
    UnknownMethod(String),
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error(transparent)]
    Serialization(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
