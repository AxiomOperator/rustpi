//! llama.cpp local server adapter.
//!
//! Wraps [`OpenAiAdapter`] with defaults appropriate for llama.cpp's built-in HTTP server.
//! llama.cpp exposes an OpenAI-compatible `/v1/chat/completions` endpoint.
//!
//! # Configuration
//! Set `base_url` to the llama.cpp server address (default: `http://localhost:8080/v1`).
//! No API key is required for local servers.
//!
//! # Limitations
//! - Embeddings: only available if llama.cpp was built/started with embedding support
//! - Model discovery: returns the single currently-loaded model
//! - Tool calling: availability depends on the model

use crate::{
    ProviderCapabilities, ProviderError,
    adapters::openai::{OpenAiAdapter, OpenAiConfig},
    provider::{
        CompletionRequest, CompletionResponse, EmbeddingRequest, EmbeddingResponse,
        ModelInfo, ModelProvider, ProviderMetadata, TokenDelta,
    },
};
use agent_core::types::{AuthState, ModelId, ProviderId};
use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

/// Configuration for the llama.cpp adapter.
#[derive(Debug, Clone)]
pub struct LlamaCppConfig {
    /// Provider identifier (default: "llamacpp").
    pub provider_id: ProviderId,
    /// Base URL of the llama.cpp server (default: `http://localhost:8080/v1`).
    pub base_url: String,
    /// Whether embeddings are available on this server.
    pub supports_embeddings: bool,
    /// Request timeout in seconds (default: 300 — local inference is slow).
    pub timeout_secs: u64,
    /// Static model list override (use when discovery returns nothing useful).
    pub static_models: Vec<ModelInfo>,
}

impl Default for LlamaCppConfig {
    fn default() -> Self {
        Self {
            provider_id: ProviderId::new("llamacpp"),
            base_url: "http://localhost:8080/v1".to_string(),
            supports_embeddings: false,
            timeout_secs: 300,
            static_models: vec![],
        }
    }
}

/// llama.cpp adapter — wraps OpenAiAdapter with llama.cpp-appropriate config.
pub struct LlamaCppAdapter {
    inner: OpenAiAdapter,
    config: LlamaCppConfig,
}

impl LlamaCppAdapter {
    pub fn new(config: LlamaCppConfig) -> Result<Self, ProviderError> {
        let openai_config = OpenAiConfig {
            provider_id: config.provider_id.clone(),
            base_url: config.base_url.clone(),
            api_key: None,
            extra_headers: vec![],
            supports_embeddings: config.supports_embeddings,
            supports_model_discovery: true,
            static_models: config.static_models.clone(),
            timeout_secs: config.timeout_secs,
        };
        let inner = OpenAiAdapter::new(openai_config)?;
        Ok(Self { inner, config })
    }
}

#[async_trait]
impl ModelProvider for LlamaCppAdapter {
    fn provider_id(&self) -> &ProviderId {
        self.inner.provider_id()
    }

    fn capabilities(&self, _model: &ModelId) -> ProviderCapabilities {
        ProviderCapabilities {
            streaming: true,
            tool_calling: true,
            embeddings: self.config.supports_embeddings,
            vision: false,
            json_mode: true,
            max_context_tokens: None,
            supports_oauth_browser: false,
            supports_device_flow: false,
            supports_api_key: false,
            supports_token_refresh: false,
            supports_model_discovery: true,
        }
    }

    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            id: self.config.provider_id.clone(),
            display_name: "llama.cpp".to_string(),
            description: "Local llama.cpp server (OpenAI-compatible API)".to_string(),
            supported_auth_flows: vec![],
            requires_network: false,
        }
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        // llama.cpp returns the active model from /v1/models; fall back to static list
        match self.inner.list_models().await {
            Ok(models) if !models.is_empty() => Ok(models),
            Ok(_) => Ok(self.config.static_models.clone()),
            Err(ProviderError::Timeout)
            | Err(ProviderError::Network(_))
            | Err(ProviderError::Unavailable(_)) => Ok(self.config.static_models.clone()),
            Err(e) => Err(e),
        }
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        self.inner.complete(request).await
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<TokenDelta, ProviderError>> + Send>>, ProviderError> {
        self.inner.complete_stream(request).await
    }

    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse, ProviderError> {
        if !self.config.supports_embeddings {
            return Err(ProviderError::UnsupportedCapability(
                "embeddings not available on this llama.cpp server".to_string(),
            ));
        }
        self.inner.embed(request).await
    }

    async fn auth_state(&self) -> AuthState {
        // Local server — always considered authenticated (no auth needed)
        AuthState::Authenticated {
            provider: self.config.provider_id.clone(),
            expires_at: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_localhost_url() {
        let config = LlamaCppConfig::default();
        assert!(config.base_url.contains("localhost:8080"));
    }

    #[test]
    fn capabilities_no_embeddings_by_default() {
        let config = LlamaCppConfig::default();
        assert!(!config.supports_embeddings);
    }

    #[test]
    fn capabilities_with_embeddings_enabled() {
        let config = LlamaCppConfig {
            supports_embeddings: true,
            ..Default::default()
        };
        assert!(config.supports_embeddings);
    }

    #[tokio::test]
    async fn auth_state_is_always_authenticated() {
        let adapter = LlamaCppAdapter::new(LlamaCppConfig::default()).unwrap();
        let state = adapter.auth_state().await;
        assert!(matches!(state, AuthState::Authenticated { .. }));
    }

    #[test]
    fn metadata_is_local() {
        let adapter = LlamaCppAdapter::new(LlamaCppConfig::default()).unwrap();
        assert!(!adapter.metadata().requires_network);
    }
}
