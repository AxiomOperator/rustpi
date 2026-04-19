//! CLI error type with exit code mapping.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CliError {
    #[error("invalid arguments: {0}")]
    InvalidArgs(String),
    #[error("auth required for provider {0}")]
    AuthRequired(String),
    #[error("auth failed: {0}")]
    AuthFailed(String),
    #[error("session not found: {0}")]
    SessionNotFound(String),
    #[error("run failed: {0}")]
    RunFailed(String),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("rpc error: {0}")]
    Rpc(#[from] rpc_api::RpcError),
    #[error("{0}")]
    Other(String),
}

impl CliError {
    /// Return an appropriate process exit code for this error.
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::InvalidArgs(_) => 2,
            Self::AuthRequired(_) | Self::AuthFailed(_) => 3,
            Self::SessionNotFound(_) => 4,
            Self::RunFailed(_) => 5,
            _ => 1,
        }
    }
}

pub type CliResult<T> = Result<T, CliError>;
