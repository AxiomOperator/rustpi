use thiserror::Error;

#[derive(Debug, Error)]
pub enum RpcError {
    #[error("protocol framing error: {0}")]
    Framing(String),
    #[error("unknown method: {0}")]
    UnknownMethod(String),
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("session not found: {0}")]
    SessionNotFound(String),
    #[error("run not found: {0}")]
    RunNotFound(String),
    #[error("invalid run state: {0}")]
    InvalidRunState(String),
    #[error("broken pipe")]
    BrokenPipe,
    #[error(transparent)]
    Serialization(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
