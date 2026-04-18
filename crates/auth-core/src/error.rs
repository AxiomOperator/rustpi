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
    #[error("CSRF state mismatch: expected {expected}, got {got}")]
    CsrfMismatch { expected: String, got: String },
    #[error("no refresh token available for provider {0}")]
    NoRefreshToken(crate::ProviderId),
    #[error("no API key available: tried env var {env_var:?} and config value")]
    NoKeyAvailable { env_var: Option<String> },
    #[error("encryption error: {0}")]
    EncryptionError(String),
    #[error("decryption error: {0}")]
    DecryptionError(String),
    #[error("device flow expired or was denied")]
    DeviceFlowExpired,
    #[error("HTTP error: {0}")]
    HttpError(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Serialization(#[from] serde_json::Error),
}
