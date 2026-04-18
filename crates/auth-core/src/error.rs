use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("authentication failed: {0}")]
    Failed(String),
    #[error("token expired for provider {0}")]
    Expired(crate::ProviderId),
    #[error("token refresh failed: {0}")]
    RefreshFailed(String),
    #[error("missing required scope: {0}")]
    MissingScope(String),
    #[error("auth flow timed out")]
    Timeout,
    #[error("token storage error: {0}")]
    Storage(String),
    #[error(transparent)]
    Serialization(#[from] serde_json::Error),
}
