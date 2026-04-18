use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("authentication required for provider {0}")]
    AuthRequired(crate::ProviderId),
    #[error("model {model} not available on provider {provider}")]
    ModelNotFound {
        provider: crate::ProviderId,
        model: crate::ModelId,
    },
    #[error("rate limited; retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },
    #[error("context length exceeded: {tokens} tokens, max {max_tokens}")]
    ContextLengthExceeded { tokens: u32, max_tokens: u32 },
    #[error("streaming error: {0}")]
    StreamError(String),
    #[error("provider returned error {status}: {message}")]
    ApiError { status: u16, message: String },
    #[error("network error: {0}")]
    Network(String),
    #[error(transparent)]
    Serialization(#[from] serde_json::Error),
    #[error("not implemented: {0}")]
    NotImplemented(String),
    #[error("authentication failed: {0}")]
    Unauthorized(String),
    #[error("access forbidden: {0}")]
    Forbidden(String),
    #[error("request timed out")]
    Timeout,
    #[error("provider unavailable: {0}")]
    Unavailable(String),
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("capability not supported by this provider: {0}")]
    UnsupportedCapability(String),
    #[error("malformed response from provider: {0}")]
    MalformedResponse(String),
    #[error("provider not configured: {0}")]
    NotConfigured(String),
}
