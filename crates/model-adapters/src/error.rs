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
}
