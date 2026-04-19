//! Built-in echo provider — streams the user's prompt back.
//!
//! Used as a zero-config fallback when no real providers are configured,
//! ensuring `rustpi run` always produces output and tests pass without an API key.

use std::pin::Pin;
use std::sync::OnceLock;

use async_trait::async_trait;
use futures::stream;
use agent_core::types::{AuthState, ModelId, ProviderId};

use crate::{
    ProviderCapabilities, ProviderError,
    provider::{
        CompletionRequest, CompletionResponse, EmbeddingRequest, EmbeddingResponse,
        FinishReason, MessageContent, ModelInfo, ModelProvider, ProviderMetadata, Role, TokenDelta,
    },
};

/// A built-in provider that echoes the last user message back.
///
/// Registered automatically when no real providers are configured.
#[derive(Debug, Default)]
pub struct EchoProvider {
    id: OnceLock<ProviderId>,
}

impl EchoProvider {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn static_provider_id() -> ProviderId {
        ProviderId::new("builtin-echo")
    }
}

#[async_trait]
impl ModelProvider for EchoProvider {
    fn provider_id(&self) -> &ProviderId {
        self.id.get_or_init(Self::static_provider_id)
    }

    fn capabilities(&self, _model: &ModelId) -> ProviderCapabilities {
        ProviderCapabilities {
            streaming: true,
            tool_calling: false,
            embeddings: false,
            vision: false,
            json_mode: false,
            max_context_tokens: None,
            supports_oauth_browser: false,
            supports_device_flow: false,
            supports_api_key: false,
            supports_token_refresh: false,
            supports_model_discovery: false,
        }
    }

    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            id: Self::static_provider_id(),
            display_name: "Built-in Echo".to_string(),
            description: "Zero-config fallback provider. Configure a real provider in config.toml."
                .to_string(),
            supported_auth_flows: vec![],
            requires_network: false,
        }
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        Ok(vec![ModelInfo {
            id: ModelId::new("echo"),
            display_name: Some("Echo".to_string()),
            context_window: None,
            supports_tools: false,
            supports_vision: false,
            supports_embeddings: false,
        }])
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        use crate::provider::{MessageContent as MC, TokenUsage};
        let text = echo_text(&request);
        let len = text.len() as u32;
        Ok(CompletionResponse {
            message: crate::provider::ChatMessage {
                role: Role::Assistant,
                content: MC::Text(text),
            },
            finish_reason: FinishReason::Stop,
            usage: TokenUsage {
                prompt_tokens: 0,
                completion_tokens: len,
                total_tokens: len,
            },
        })
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<TokenDelta, ProviderError>> + Send>>, ProviderError>
    {
        let text = echo_text(&request);
        // Stream word-by-word so the token streaming path is exercised.
        let words: Vec<String> = text
            .split_whitespace()
            .enumerate()
            .map(|(i, w)| if i == 0 { w.to_string() } else { format!(" {}", w) })
            .collect();

        let chunks: Vec<Result<TokenDelta, ProviderError>> = words
            .into_iter()
            .map(|w| {
                Ok(TokenDelta {
                    text: Some(w),
                    tool_call: None,
                    finish_reason: None,
                })
            })
            .chain(std::iter::once(Ok(TokenDelta {
                text: None,
                tool_call: None,
                finish_reason: Some(FinishReason::Stop),
            })))
            .collect();

        Ok(Box::pin(stream::iter(chunks)))
    }

    async fn embed(&self, _request: EmbeddingRequest) -> Result<EmbeddingResponse, ProviderError> {
        Err(ProviderError::UnsupportedCapability(
            "EchoProvider does not support embeddings".into(),
        ))
    }

    async fn auth_state(&self) -> AuthState {
        AuthState::Authenticated {
            provider: Self::static_provider_id(),
            expires_at: None,
        }
    }
}

fn echo_text(request: &CompletionRequest) -> String {
    // Find the last user message to echo back.
    let user_text = request
        .messages
        .iter()
        .rev()
        .find_map(|m| {
            if matches!(m.role, Role::User) {
                match &m.content {
                    MessageContent::Text(t) => Some(t.clone()),
                    _ => None,
                }
            } else {
                None
            }
        })
        .unwrap_or_else(|| "(no input)".to_string());

    format!(
        "[builtin-echo] No providers configured — echoing your prompt:\n\n{}\n\n\
         Tip: add a [[providers]] entry to ~/.config/rustpi/config.toml to use a real model.",
        user_text
    )
}
