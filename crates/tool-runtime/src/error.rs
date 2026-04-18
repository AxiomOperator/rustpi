use thiserror::Error;

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("tool '{0}' not registered")]
    NotFound(String),
    #[error("tool execution timed out after {0}s")]
    Timeout(u64),
    #[error("tool execution was cancelled")]
    Cancelled,
    #[error("tool execution denied by policy: {0}")]
    PolicyDenied(String),
    #[error("invalid arguments for tool '{tool}': {reason}")]
    InvalidArguments { tool: String, reason: String },
    #[error("subprocess failed with exit code {code}: {stderr}")]
    SubprocessFailed { code: i32, stderr: String },
    #[error("path traversal attempt blocked: {0}")]
    PathTraversal(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Serialization(#[from] serde_json::Error),
}
