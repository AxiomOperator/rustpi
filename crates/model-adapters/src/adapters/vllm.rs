//! vLLM local/remote server adapter.
//!
//! Wraps [`OpenAiAdapter`] with defaults appropriate for vLLM's OpenAI-compatible HTTP server.
//! vLLM exposes `/v1/chat/completions`, `/v1/models`, and `/v1/embeddings` endpoints.
//!
//! # Configuration
//! Set `base_url` to the vLLM server address (default: `http://localhost:8000/v1`).
//! An optional API key can be configured for deployments that require auth.
//!
//! # Notes
//! - Embeddings are enabled by default (vLLM supports embedding models)
//! - Model discovery works — vLLM returns all loaded models from `/v1/models`
//! - vLLM is batch-oriented and may queue requests; default timeout is 600s

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

/// Configuration for the vLLM adapter.
#[derive(Debug, Clone)]
pub struct VllmConfig {
    /// Provider identifier (default: "vllm").
    pub provider_id: ProviderId,
    /// Base URL of the vLLM server (default: `http://localhost:8000/v1`).
    pub base_url: String,
    /// Optional API key (vLLM supports optional auth).
    pub api_key: Option<String>,
    /// Whether embeddings are available on this server.
    pub supports_embeddings: bool,
    /// Request timeout in seconds (default: 600 — vLLM may queue requests).
    pub timeout_secs: u64,
    /// Static model list override (use when discovery returns nothing useful).
    pub static_models: Vec<ModelInfo>,
}

impl Default for VllmConfig {
    fn default() -> Self {
        Self {
            provider_id: ProviderId::new("vllm"),
            base_url: "http://localhost:8000/v1".to_string(),
            api_key: None,
            supports_embeddings: true,
            timeout_secs: 600,
            static_models: vec![],
        }
    }
}

/// vLLM adapter — wraps OpenAiAdapter with vLLM-appropriate config.
pub struct VllmAdapter {
    inner: OpenAiAdapter,
    config: VllmConfig,
}

impl VllmAdapter {
    pub fn new(config: VllmConfig) -> Result<Self, ProviderError> {
        let openai_config = OpenAiConfig {
            provider_id: config.provider_id.clone(),
            base_url: config.base_url.clone(),
            api_key: config.api_key.clone(),
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
impl ModelProvider for VllmAdapter {
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
            supports_api_key: self.config.api_key.is_some(),
            supports_token_refresh: false,
            supports_model_discovery: true,
        }
    }

    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            id: self.config.provider_id.clone(),
            display_name: "vLLM".to_string(),
            description: "vLLM server (OpenAI-compatible API)".to_string(),
            supported_auth_flows: vec![],
            requires_network: false,
        }
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
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
                "embeddings not available on this vLLM server".to_string(),
            ));
        }
        self.inner.embed(request).await
    }

    async fn auth_state(&self) -> AuthState {
        // Local server — always considered authenticated (no auth needed beyond optional API key)
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
        let config = VllmConfig::default();
        assert!(config.base_url.contains("localhost:8000"));
    }

    #[test]
    fn embeddings_enabled_by_default() {
        let config = VllmConfig::default();
        assert!(config.supports_embeddings);
    }

    #[test]
    fn embeddings_can_be_disabled() {
        let config = VllmConfig {
            supports_embeddings: false,
            ..Default::default()
        };
        assert!(!config.supports_embeddings);
    }

    #[tokio::test]
    async fn auth_state_is_always_authenticated() {
        let adapter = VllmAdapter::new(VllmConfig::default()).unwrap();
        let state = adapter.auth_state().await;
        assert!(matches!(state, AuthState::Authenticated { .. }));
    }

    #[test]
    fn metadata_is_local() {
        let adapter = VllmAdapter::new(VllmConfig::default()).unwrap();
        assert!(!adapter.metadata().requires_network);
    }
}
