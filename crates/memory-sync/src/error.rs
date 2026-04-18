use thiserror::Error;

#[derive(Debug, Error)]
pub enum MemorySyncError {
    #[error("vault not found at path: {0}")]
    VaultNotFound(String),
    #[error("document '{0}' is read-only")]
    ReadOnly(String),
    #[error("document '{0}' requires approval before writing")]
    ApprovalRequired(String),
    #[error("sync conflict on document '{doc}': {reason}")]
    Conflict { doc: String, reason: String },
    #[error("malformed markdown in '{0}'")]
    MalformedMarkdown(String),
    #[error("path traversal denied: {0}")]
    PathTraversal(String),
    #[error("vault initialization error: {0}")]
    Init(String),
    #[error("qdrant error: {0}")]
    Qdrant(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
