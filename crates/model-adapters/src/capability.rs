//! Provider capability metadata.

use serde::{Deserialize, Serialize};

/// Capabilities advertised by a provider/model combination.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderCapabilities {
    pub streaming: bool,
    pub tool_calling: bool,
    pub embeddings: bool,
    pub vision: bool,
    pub json_mode: bool,
    /// Maximum context tokens supported.
    pub max_context_tokens: Option<u32>,

    // Auth capability flags
    pub supports_oauth_browser: bool,
    pub supports_device_flow: bool,
    pub supports_api_key: bool,
    pub supports_token_refresh: bool,

    // Model discovery
    pub supports_model_discovery: bool,
}

impl ProviderCapabilities {
    /// Returns true if any auth flow is supported.
    pub fn requires_auth(&self) -> bool {
        self.supports_oauth_browser || self.supports_device_flow || self.supports_api_key
    }

    /// Returns an OpenAI-compatible capability set (common defaults).
    pub fn openai_compatible() -> Self {
        Self {
            streaming: true,
            tool_calling: true,
            embeddings: true,
            vision: false,
            json_mode: true,
            max_context_tokens: Some(128_000),
            supports_oauth_browser: false,
            supports_device_flow: false,
            supports_api_key: true,
            supports_token_refresh: false,
            supports_model_discovery: true,
        }
    }

    /// Returns a local/no-auth capability set (llama.cpp, vLLM).
    pub fn local_no_auth() -> Self {
        Self {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{
        EmbeddingRequest, EmbeddingResponse, ModelInfo, ProviderMetadata, TokenUsage,
    };
    use crate::registry::ProviderRegistry;
    use crate::{ModelId, ProviderId};
    use agent_core::types::AuthFlow;
    use std::sync::Arc;

    #[test]
    fn requires_auth_false_when_no_auth() {
        let caps = ProviderCapabilities::default();
        assert!(!caps.requires_auth());
    }

    #[test]
    fn requires_auth_true_for_api_key() {
        let caps = ProviderCapabilities {
            supports_api_key: true,
            ..Default::default()
        };
        assert!(caps.requires_auth());
    }

    #[test]
    fn requires_auth_true_for_oauth_browser() {
        let caps = ProviderCapabilities {
            supports_oauth_browser: true,
            ..Default::default()
        };
        assert!(caps.requires_auth());
    }

    #[test]
    fn requires_auth_true_for_device_flow() {
        let caps = ProviderCapabilities {
            supports_device_flow: true,
            ..Default::default()
        };
        assert!(caps.requires_auth());
    }

    #[test]
    fn openai_compatible_has_expected_caps() {
        let caps = ProviderCapabilities::openai_compatible();
        assert!(caps.streaming);
        assert!(caps.tool_calling);
        assert!(caps.supports_api_key);
        assert!(caps.supports_model_discovery);
        assert!(!caps.supports_oauth_browser);
        assert!(!caps.supports_device_flow);
        assert!(caps.requires_auth());
    }

    #[test]
    fn local_no_auth_has_streaming_no_auth() {
        let caps = ProviderCapabilities::local_no_auth();
        assert!(caps.streaming);
        assert!(!caps.requires_auth());
        assert!(!caps.supports_model_discovery);
    }

    #[test]
    fn model_info_roundtrip() {
        let info = ModelInfo {
            id: ModelId::new("gpt-4o"),
            display_name: Some("GPT-4o".to_string()),
            context_window: Some(128_000),
            supports_tools: true,
            supports_vision: true,
            supports_embeddings: false,
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: ModelInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id.0, "gpt-4o");
        assert_eq!(back.context_window, Some(128_000));
        assert!(back.supports_tools);
    }

    #[test]
    fn provider_metadata_roundtrip() {
        let meta = ProviderMetadata {
            id: ProviderId::new("openai"),
            display_name: "OpenAI".to_string(),
            description: "OpenAI API".to_string(),
            supported_auth_flows: vec![AuthFlow::ApiKey],
            requires_network: true,
        };
        let json = serde_json::to_string(&meta).unwrap();
        let back: ProviderMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id.0, "openai");
        assert!(back.requires_network);
        assert_eq!(back.supported_auth_flows.len(), 1);
    }

    #[test]
    fn embedding_request_response_roundtrip() {
        let req = EmbeddingRequest {
            model: ModelId::new("text-embedding-3-small"),
            inputs: vec!["hello world".to_string()],
            dimensions: Some(512),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: EmbeddingRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.dimensions, Some(512));
        assert_eq!(back.inputs[0], "hello world");

        let resp = EmbeddingResponse {
            embeddings: vec![vec![0.1, 0.2, 0.3]],
            model: ModelId::new("text-embedding-3-small"),
            usage: TokenUsage {
                prompt_tokens: 2,
                completion_tokens: 0,
                total_tokens: 2,
            },
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: EmbeddingResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.embeddings[0], vec![0.1f32, 0.2f32, 0.3f32]);
    }

    #[test]
    fn registry_register_and_get() {
        use crate::provider::{CompletionRequest, CompletionResponse, TokenDelta};
        use agent_core::types::AuthState;

        struct MockProvider {
            id: ProviderId,
            caps: ProviderCapabilities,
        }

        #[async_trait::async_trait]
        impl crate::provider::ModelProvider for MockProvider {
            fn provider_id(&self) -> &ProviderId {
                &self.id
            }
            fn capabilities(&self, _model: &ModelId) -> ProviderCapabilities {
                self.caps.clone()
            }
            fn metadata(&self) -> ProviderMetadata {
                ProviderMetadata {
                    id: self.id.clone(),
                    display_name: "Mock".to_string(),
                    description: "Test mock provider".to_string(),
                    supported_auth_flows: vec![],
                    requires_network: false,
                }
            }
            async fn list_models(&self) -> Result<Vec<ModelInfo>, crate::ProviderError> {
                Ok(vec![])
            }
            async fn complete(
                &self,
                _req: CompletionRequest,
            ) -> Result<CompletionResponse, crate::ProviderError> {
                Err(crate::ProviderError::NotImplemented("mock".into()))
            }
            async fn complete_stream(
                &self,
                _req: CompletionRequest,
            ) -> Result<
                std::pin::Pin<
                    Box<
                        dyn futures::Stream<Item = Result<TokenDelta, crate::ProviderError>>
                            + Send,
                    >,
                >,
                crate::ProviderError,
            > {
                Err(crate::ProviderError::NotImplemented("mock".into()))
            }
            async fn embed(
                &self,
                _req: EmbeddingRequest,
            ) -> Result<EmbeddingResponse, crate::ProviderError> {
                Err(crate::ProviderError::NotImplemented("mock".into()))
            }
            async fn auth_state(&self) -> AuthState {
                AuthState::Unauthenticated
            }
        }

        let mut registry = ProviderRegistry::new();
        assert!(registry.is_empty());

        let provider = Arc::new(MockProvider {
            id: ProviderId::new("mock"),
            caps: ProviderCapabilities::default(),
        });
        registry.register(provider);

        assert_eq!(registry.len(), 1);
        let id = ProviderId::new("mock");
        assert!(registry.get(&id).is_some());
        assert_eq!(registry.list().len(), 1);
    }

    #[test]
    fn registry_list_returns_correct_ids() {
        use crate::provider::{CompletionRequest, CompletionResponse, TokenDelta};
        use agent_core::types::AuthState;

        struct MockProvider2 {
            id: ProviderId,
        }

        #[async_trait::async_trait]
        impl crate::provider::ModelProvider for MockProvider2 {
            fn provider_id(&self) -> &ProviderId {
                &self.id
            }
            fn capabilities(&self, _model: &ModelId) -> ProviderCapabilities {
                ProviderCapabilities::default()
            }
            fn metadata(&self) -> ProviderMetadata {
                ProviderMetadata {
                    id: self.id.clone(),
                    display_name: self.id.0.clone(),
                    description: String::new(),
                    supported_auth_flows: vec![],
                    requires_network: false,
                }
            }
            async fn list_models(&self) -> Result<Vec<ModelInfo>, crate::ProviderError> {
                Ok(vec![])
            }
            async fn complete(
                &self,
                _req: CompletionRequest,
            ) -> Result<CompletionResponse, crate::ProviderError> {
                Err(crate::ProviderError::NotImplemented("mock2".into()))
            }
            async fn complete_stream(
                &self,
                _req: CompletionRequest,
            ) -> Result<
                std::pin::Pin<
                    Box<
                        dyn futures::Stream<Item = Result<TokenDelta, crate::ProviderError>>
                            + Send,
                    >,
                >,
                crate::ProviderError,
            > {
                Err(crate::ProviderError::NotImplemented("mock2".into()))
            }
            async fn embed(
                &self,
                _req: EmbeddingRequest,
            ) -> Result<EmbeddingResponse, crate::ProviderError> {
                Err(crate::ProviderError::NotImplemented("mock2".into()))
            }
            async fn auth_state(&self) -> AuthState {
                AuthState::Unauthenticated
            }
        }

        let mut registry = ProviderRegistry::new();
        for name in ["alpha", "beta", "gamma"] {
            registry.register(Arc::new(MockProvider2 {
                id: ProviderId::new(name),
            }));
        }

        assert_eq!(registry.len(), 3);
        let ids: Vec<String> = registry.list().into_iter().map(|p| p.0.clone()).collect();
        assert!(ids.contains(&"alpha".to_string()));
        assert!(ids.contains(&"beta".to_string()));
        assert!(ids.contains(&"gamma".to_string()));
    }
}

