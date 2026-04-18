//! GitHub Copilot adapter.
//!
//! Uses GitHub device flow for authentication and wraps the Copilot Chat API,
//! which is OpenAI-compatible at the wire level.
//!
//! # Authentication
//! 1. Authenticate via GitHub device flow (client_id for GitHub Copilot)
//! 2. Obtain a GitHub OAuth token
//! 3. Exchange for a short-lived Copilot API token via `GET https://api.github.com/copilot_internal/v2/token`
//! 4. Use Copilot token in `Authorization: Bearer` header
//!
//! The Copilot token expires every ~30 minutes. [`CopilotAdapter`] caches the token
//! and auto-refreshes on 401 responses.
//!
//! # Token store
//! This adapter uses an in-memory token cache (`tokio::sync::RwLock`). For persistence,
//! swap the internal store for an `EncryptedFileTokenStore` from `auth-core`.
//!
//! # Supported models (static list — Copilot does not expose /models)
//! - gpt-4o
//! - gpt-4
//! - gpt-3.5-turbo
//! - claude-3.5-sonnet (if available to account)
//!
//! # Limitations
//! - Embeddings: not supported
//! - Model discovery: static list only

use crate::{
    ProviderCapabilities, ProviderError,
    adapters::openai::{OpenAiAdapter, OpenAiConfig},
    provider::{
        CompletionRequest, CompletionResponse, EmbeddingRequest, EmbeddingResponse,
        ModelInfo, ModelProvider, ProviderMetadata, TokenDelta,
    },
    wire::map_api_error,
};
use agent_core::types::{AuthFlow, AuthState, ModelId, ProviderId};
use async_trait::async_trait;
use futures::Stream;
use reqwest::Client;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Configuration for the GitHub Copilot adapter.
#[derive(Debug, Clone)]
pub struct CopilotConfig {
    /// Provider identifier (default: "copilot").
    pub provider_id: ProviderId,
    /// GitHub OAuth token obtained via device flow.
    pub github_token: Option<String>,
    /// Currently-valid Copilot API token (short-lived, auto-refreshed from `github_token`).
    pub copilot_token: Option<String>,
    /// Copilot API base URL.
    pub base_url: String,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
}

impl Default for CopilotConfig {
    fn default() -> Self {
        Self {
            provider_id: ProviderId::new("copilot"),
            github_token: None,
            copilot_token: None,
            base_url: "https://api.githubcopilot.com".to_string(),
            timeout_secs: 120,
        }
    }
}

/// GitHub Copilot adapter.
///
/// Delegates HTTP calls to a dynamically-constructed `OpenAiAdapter` seeded with
/// the current Copilot token. On 401 responses the token is cleared and one
/// automatic re-exchange is attempted before propagating the error.
pub struct CopilotAdapter {
    config: CopilotConfig,
    client: Client,
    /// Cached short-lived Copilot API token.
    copilot_token: Arc<RwLock<Option<String>>>,
}

impl CopilotAdapter {
    pub fn new(config: CopilotConfig) -> Result<Self, ProviderError> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| ProviderError::NotConfigured(e.to_string()))?;

        // Pre-seed the lock from config if a copilot token was provided.
        let initial_token = config.copilot_token.clone();
        Ok(Self {
            config,
            client,
            copilot_token: Arc::new(RwLock::new(initial_token)),
        })
    }

    /// Build an `OpenAiAdapter` pre-configured with the given Copilot bearer token.
    fn build_openai_adapter(&self, token: &str) -> Result<OpenAiAdapter, ProviderError> {
        let openai_config = OpenAiConfig {
            provider_id: self.config.provider_id.clone(),
            base_url: self.config.base_url.clone(),
            api_key: Some(token.to_string()),
            extra_headers: vec![
                ("Editor-Version".to_string(), "rustpi/0.1".to_string()),
                ("Copilot-Integration-Id".to_string(), "vscode-chat".to_string()),
            ],
            supports_embeddings: false,
            supports_model_discovery: false,
            static_models: static_copilot_models(),
            timeout_secs: self.config.timeout_secs,
        };
        OpenAiAdapter::new(openai_config)
    }

    /// Return a valid Copilot token, exchanging the GitHub token if needed.
    async fn get_or_refresh_token(&self) -> Result<String, ProviderError> {
        // Fast path — cached token still valid.
        {
            let guard = self.copilot_token.read().await;
            if let Some(tok) = guard.as_ref() {
                return Ok(tok.clone());
            }
        }

        // Slow path — exchange GitHub token for a Copilot token.
        let github_token = self
            .config
            .github_token
            .as_deref()
            .ok_or_else(|| ProviderError::AuthRequired(self.config.provider_id.clone()))?;

        let new_token = Self::exchange_github_token(github_token, &self.client).await?;
        {
            let mut guard = self.copilot_token.write().await;
            *guard = Some(new_token.clone());
        }
        Ok(new_token)
    }

    /// Clear the cached Copilot token (called on 401 to force re-exchange).
    async fn invalidate_token(&self) {
        let mut guard = self.copilot_token.write().await;
        *guard = None;
    }

    /// Exchange a GitHub OAuth token for a short-lived Copilot API token.
    async fn exchange_github_token(
        github_token: &str,
        client: &Client,
    ) -> Result<String, ProviderError> {
        #[derive(serde::Deserialize)]
        struct CopilotTokenResponse {
            token: String,
        }

        let response = client
            .get("https://api.github.com/copilot_internal/v2/token")
            .header("Authorization", format!("token {}", github_token))
            .header("User-Agent", "rustpi/0.1")
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(map_api_error(status, &body));
        }

        let token_resp: CopilotTokenResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::MalformedResponse(e.to_string()))?;

        Ok(token_resp.token)
    }
}

/// Static model list for GitHub Copilot (Copilot does not expose a /models endpoint).
pub fn static_copilot_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            id: ModelId::new("gpt-4o"),
            display_name: Some("GPT-4o".to_string()),
            context_window: Some(128_000),
            supports_tools: true,
            supports_vision: true,
            supports_embeddings: false,
        },
        ModelInfo {
            id: ModelId::new("gpt-4"),
            display_name: Some("GPT-4".to_string()),
            context_window: Some(8_192),
            supports_tools: true,
            supports_vision: false,
            supports_embeddings: false,
        },
        ModelInfo {
            id: ModelId::new("gpt-3.5-turbo"),
            display_name: Some("GPT-3.5 Turbo".to_string()),
            context_window: Some(16_385),
            supports_tools: true,
            supports_vision: false,
            supports_embeddings: false,
        },
        ModelInfo {
            id: ModelId::new("claude-3.5-sonnet"),
            display_name: Some("Claude 3.5 Sonnet (via Copilot)".to_string()),
            context_window: Some(200_000),
            supports_tools: true,
            supports_vision: true,
            supports_embeddings: false,
        },
    ]
}

#[async_trait]
impl ModelProvider for CopilotAdapter {
    fn provider_id(&self) -> &ProviderId {
        &self.config.provider_id
    }

    fn capabilities(&self, _model: &ModelId) -> ProviderCapabilities {
        ProviderCapabilities {
            streaming: true,
            tool_calling: true,
            embeddings: false,
            vision: false,
            json_mode: true,
            max_context_tokens: None,
            supports_oauth_browser: false,
            supports_device_flow: true,
            supports_api_key: false,
            supports_token_refresh: true,
            supports_model_discovery: false,
        }
    }

    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            id: self.config.provider_id.clone(),
            display_name: "GitHub Copilot".to_string(),
            description: "GitHub Copilot Chat (OpenAI-compatible, OAuth device flow)".to_string(),
            supported_auth_flows: vec![AuthFlow::DeviceCode],
            requires_network: true,
        }
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        Ok(static_copilot_models())
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let token = self.get_or_refresh_token().await?;
        let adapter = self.build_openai_adapter(&token)?;

        match adapter.complete(request.clone()).await {
            Err(ProviderError::Unauthorized(_)) => {
                // Token may have expired — invalidate and retry once.
                self.invalidate_token().await;
                let fresh_token = self.get_or_refresh_token().await?;
                let retry_adapter = self.build_openai_adapter(&fresh_token)?;
                retry_adapter.complete(request).await
            }
            other => other,
        }
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<TokenDelta, ProviderError>> + Send>>, ProviderError>
    {
        let token = self.get_or_refresh_token().await?;
        let adapter = self.build_openai_adapter(&token)?;
        adapter.complete_stream(request).await
    }

    async fn embed(&self, _request: EmbeddingRequest) -> Result<EmbeddingResponse, ProviderError> {
        Err(ProviderError::UnsupportedCapability(
            "copilot does not support embeddings".to_string(),
        ))
    }

    async fn auth_state(&self) -> AuthState {
        if self.config.github_token.is_some() {
            AuthState::Authenticated {
                provider: self.config.provider_id.clone(),
                expires_at: None,
            }
        } else {
            AuthState::Unauthenticated
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_models_are_nonempty() {
        assert!(!static_copilot_models().is_empty());
    }

    #[tokio::test]
    async fn auth_state_unauthenticated_without_token() {
        let adapter = CopilotAdapter::new(CopilotConfig::default()).unwrap();
        let state = adapter.auth_state().await;
        assert!(matches!(state, AuthState::Unauthenticated));
    }

    #[tokio::test]
    async fn auth_state_authenticated_with_github_token() {
        let config = CopilotConfig {
            github_token: Some("ghp_test_token".to_string()),
            ..Default::default()
        };
        let adapter = CopilotAdapter::new(config).unwrap();
        let state = adapter.auth_state().await;
        assert!(matches!(state, AuthState::Authenticated { .. }));
    }

    #[tokio::test]
    async fn embed_returns_unsupported_capability() {
        let adapter = CopilotAdapter::new(CopilotConfig::default()).unwrap();
        let result = adapter
            .embed(EmbeddingRequest {
                model: ModelId::new("gpt-4o"),
                inputs: vec!["test".to_string()],
                dimensions: None,
            })
            .await;
        assert!(matches!(result, Err(ProviderError::UnsupportedCapability(_))));
    }

    #[tokio::test]
    async fn list_models_returns_static_list() {
        let adapter = CopilotAdapter::new(CopilotConfig::default()).unwrap();
        let models = adapter.list_models().await.unwrap();
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.id.to_string().contains("gpt-4")));
    }

    #[test]
    fn capabilities_has_device_flow() {
        let adapter = CopilotAdapter::new(CopilotConfig::default()).unwrap();
        let caps = adapter.capabilities(&ModelId::new("gpt-4o"));
        assert!(caps.supports_device_flow);
        assert!(!caps.embeddings);
        assert!(caps.streaming);
    }

    #[test]
    fn metadata_requires_network() {
        let adapter = CopilotAdapter::new(CopilotConfig::default()).unwrap();
        assert!(adapter.metadata().requires_network);
    }
}
