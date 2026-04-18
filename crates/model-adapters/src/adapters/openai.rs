//! OpenAI-compatible adapter.
//!
//! Works with OpenAI, Azure OpenAI (with appropriate base URL), and any
//! OpenAI-compatible server (vLLM, llama.cpp, LM Studio, etc.).
//! Configure via [`OpenAiConfig`].

use crate::{
    ProviderCapabilities, ProviderError,
    provider::{
        ChatMessage, CompletionRequest, CompletionResponse, EmbeddingRequest, EmbeddingResponse,
        FinishReason, MessageContent, ModelInfo, ModelProvider, ProviderMetadata, Role, TokenDelta,
        TokenUsage,
    },
    wire::{
        WireChatMessage, WireChatRequest, WireChatResponse, WireEmbeddingRequest, WireModelList,
        WireStreamChunk, WireUsage, map_api_error,
    },
};
use agent_core::types::{AuthFlow, AuthState, ModelId, ProviderId};
use async_trait::async_trait;
use futures::Stream;
use reqwest::{Client, RequestBuilder};
use std::pin::Pin;

/// Configuration for the OpenAI-compatible adapter.
#[derive(Debug, Clone)]
pub struct OpenAiConfig {
    /// Provider identifier (e.g. "openai", "azure", "local").
    pub provider_id: ProviderId,
    /// API base URL, without trailing slash (e.g. "https://api.openai.com/v1").
    pub base_url: String,
    /// Bearer token / API key, if required.
    pub api_key: Option<String>,
    /// Extra headers to send with every request (e.g. custom org headers).
    pub extra_headers: Vec<(String, String)>,
    /// Whether this provider supports embeddings.
    pub supports_embeddings: bool,
    /// Whether this provider supports model discovery.
    pub supports_model_discovery: bool,
    /// Static model list to use when discovery is disabled.
    pub static_models: Vec<ModelInfo>,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
}

impl OpenAiConfig {
    /// Standard OpenAI config requiring an API key.
    pub fn openai(api_key: impl Into<String>) -> Self {
        Self {
            provider_id: ProviderId::new("openai"),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: Some(api_key.into()),
            extra_headers: vec![],
            supports_embeddings: true,
            supports_model_discovery: true,
            static_models: vec![],
            timeout_secs: 120,
        }
    }
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            provider_id: ProviderId::new("openai"),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: None,
            extra_headers: vec![],
            supports_embeddings: true,
            supports_model_discovery: true,
            static_models: vec![],
            timeout_secs: 120,
        }
    }
}

pub struct OpenAiAdapter {
    config: OpenAiConfig,
    client: Client,
}

impl OpenAiAdapter {
    pub fn new(config: OpenAiConfig) -> Result<Self, ProviderError> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| ProviderError::NotConfigured(e.to_string()))?;
        Ok(Self { config, client })
    }

    fn apply_auth(&self, rb: RequestBuilder) -> RequestBuilder {
        if let Some(key) = &self.config.api_key {
            rb.bearer_auth(key)
        } else {
            rb
        }
    }

    fn apply_extra_headers(&self, mut rb: RequestBuilder) -> RequestBuilder {
        for (k, v) in &self.config.extra_headers {
            rb = rb.header(k.as_str(), v.as_str());
        }
        rb
    }

    fn build_request(&self, rb: RequestBuilder) -> RequestBuilder {
        self.apply_extra_headers(self.apply_auth(rb))
    }

    async fn check_response_error(
        response: reqwest::Response,
    ) -> Result<reqwest::Response, ProviderError> {
        let status = response.status().as_u16();
        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(map_api_error(status, &body));
        }
        Ok(response)
    }

    fn convert_message_to_wire(msg: &ChatMessage) -> WireChatMessage {
        let role = match msg.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        }
        .to_string();

        let (content, tool_call_id) = match &msg.content {
            MessageContent::Text(t) => (serde_json::Value::String(t.clone()), None),
            MessageContent::ToolResult { call_id, output } => {
                (output.clone(), Some(call_id.clone()))
            }
        };

        WireChatMessage {
            role,
            content,
            tool_calls: None,
            tool_call_id,
        }
    }

    fn convert_wire_to_message(wire: &WireChatMessage) -> ChatMessage {
        let role = match wire.role.as_str() {
            "system" => Role::System,
            "assistant" => Role::Assistant,
            "tool" => Role::Tool,
            _ => Role::User,
        };
        let content = match &wire.content {
            serde_json::Value::String(s) => MessageContent::Text(s.clone()),
            other => MessageContent::Text(other.to_string()),
        };
        ChatMessage { role, content }
    }

    fn convert_finish_reason(s: Option<&str>) -> FinishReason {
        match s {
            Some("stop") | None => FinishReason::Stop,
            Some("length") => FinishReason::Length,
            Some("tool_calls") => FinishReason::ToolCalls,
            Some("content_filter") => FinishReason::ContentFilter,
            _ => FinishReason::Stop,
        }
    }
}

#[async_trait]
impl ModelProvider for OpenAiAdapter {
    fn provider_id(&self) -> &ProviderId {
        &self.config.provider_id
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
            supports_api_key: true,
            supports_token_refresh: false,
            supports_model_discovery: self.config.supports_model_discovery,
        }
    }

    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            id: self.config.provider_id.clone(),
            display_name: "OpenAI-compatible".to_string(),
            description: "OpenAI API-compatible provider (OpenAI, Azure, local servers)"
                .to_string(),
            supported_auth_flows: vec![AuthFlow::ApiKey],
            requires_network: true,
        }
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        if !self.config.supports_model_discovery {
            return Ok(self.config.static_models.clone());
        }

        let url = format!("{}/models", self.config.base_url);
        let rb = self.client.get(&url);
        let response = self
            .build_request(rb)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    ProviderError::Timeout
                } else {
                    ProviderError::Network(e.to_string())
                }
            })?;
        let response = Self::check_response_error(response).await?;
        let model_list: WireModelList = response
            .json()
            .await
            .map_err(|e| ProviderError::MalformedResponse(e.to_string()))?;

        Ok(model_list
            .data
            .into_iter()
            .map(|m| ModelInfo {
                id: ModelId::new(&m.id),
                display_name: Some(m.id.clone()),
                context_window: None,
                supports_tools: m.id.contains("gpt") || m.id.contains("claude"),
                supports_vision: m.id.contains("vision") || m.id.contains("4o"),
                supports_embeddings: m.id.contains("embedding"),
            })
            .collect())
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        let url = format!("{}/chat/completions", self.config.base_url);
        let wire_req = WireChatRequest {
            model: request.model.to_string(),
            messages: request
                .messages
                .iter()
                .map(Self::convert_message_to_wire)
                .collect(),
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            stream: false,
            tools: request.tools,
        };

        let rb = self.client.post(&url).json(&wire_req);
        let response = self
            .build_request(rb)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    ProviderError::Timeout
                } else {
                    ProviderError::Network(e.to_string())
                }
            })?;
        let response = Self::check_response_error(response).await?;
        let wire_resp: WireChatResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::MalformedResponse(e.to_string()))?;

        let choice = wire_resp
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| ProviderError::MalformedResponse("empty choices".to_string()))?;
        let usage = wire_resp.usage.unwrap_or(WireUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        });

        Ok(CompletionResponse {
            message: Self::convert_wire_to_message(&choice.message),
            finish_reason: Self::convert_finish_reason(choice.finish_reason.as_deref()),
            usage: TokenUsage {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                total_tokens: usage.total_tokens,
            },
        })
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<TokenDelta, ProviderError>> + Send>>, ProviderError>
    {
        let url = format!("{}/chat/completions", self.config.base_url);
        let wire_req = WireChatRequest {
            model: request.model.to_string(),
            messages: request
                .messages
                .iter()
                .map(Self::convert_message_to_wire)
                .collect(),
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            stream: true,
            tools: request.tools,
        };

        let rb = self.client.post(&url).json(&wire_req);
        let response = self
            .build_request(rb)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    ProviderError::Timeout
                } else {
                    ProviderError::Network(e.to_string())
                }
            })?;
        let response = Self::check_response_error(response).await?;

        let byte_stream = response.bytes_stream();
        let delta_stream = parse_sse_stream(byte_stream);
        Ok(Box::pin(delta_stream))
    }

    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse, ProviderError> {
        if !self.config.supports_embeddings {
            return Err(ProviderError::UnsupportedCapability(
                "embeddings".to_string(),
            ));
        }

        let url = format!("{}/embeddings", self.config.base_url);
        let wire_req = WireEmbeddingRequest {
            model: request.model.to_string(),
            input: request.inputs,
            dimensions: request.dimensions,
        };

        let rb = self.client.post(&url).json(&wire_req);
        let response = self
            .build_request(rb)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    ProviderError::Timeout
                } else {
                    ProviderError::Network(e.to_string())
                }
            })?;
        let response = Self::check_response_error(response).await?;
        let wire_resp: crate::wire::WireEmbeddingResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::MalformedResponse(e.to_string()))?;

        let mut data = wire_resp.data;
        data.sort_by_key(|d| d.index);

        Ok(EmbeddingResponse {
            embeddings: data.into_iter().map(|d| d.embedding).collect(),
            model: ModelId::new(&wire_resp.model),
            usage: TokenUsage {
                prompt_tokens: wire_resp.usage.prompt_tokens,
                completion_tokens: wire_resp.usage.completion_tokens,
                total_tokens: wire_resp.usage.total_tokens,
            },
        })
    }

    async fn auth_state(&self) -> AuthState {
        if self.config.api_key.is_some() {
            AuthState::Authenticated {
                provider: self.config.provider_id.clone(),
                expires_at: None,
            }
        } else {
            AuthState::Unauthenticated
        }
    }
}

/// Parse an SSE byte stream into a stream of [`TokenDelta`]s.
fn parse_sse_stream<S>(
    byte_stream: S,
) -> impl Stream<Item = Result<TokenDelta, ProviderError>> + Send
where
    S: futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
{
    use futures::StreamExt;

    async_stream::stream! {
        let mut buffer = String::new();

        tokio::pin!(byte_stream);
        while let Some(chunk) = byte_stream.next().await {
            let chunk = match chunk {
                Ok(c) => c,
                Err(e) => {
                    yield Err(ProviderError::StreamError(e.to_string()));
                    return;
                }
            };

            let text = match std::str::from_utf8(&chunk) {
                Ok(t) => t.to_string(),
                Err(e) => {
                    yield Err(ProviderError::StreamError(format!("UTF-8 error: {e}")));
                    return;
                }
            };

            buffer.push_str(&text);

            // Process complete SSE events (terminated by blank line).
            while let Some(event_end) = find_event_end(&buffer) {
                let event = buffer[..event_end].to_string();
                buffer = buffer[event_end..]
                    .trim_start_matches('\n')
                    .trim_start_matches('\r')
                    .to_string();

                for line in event.lines() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        if data.trim() == "[DONE]" {
                            return;
                        }
                        match serde_json::from_str::<WireStreamChunk>(data) {
                            Ok(chunk) => {
                                if let Some(choice) = chunk.choices.into_iter().next() {
                                    let finish_reason =
                                        choice.finish_reason.as_deref().map(|r| match r {
                                            "stop" => FinishReason::Stop,
                                            "length" => FinishReason::Length,
                                            "tool_calls" => FinishReason::ToolCalls,
                                            "content_filter" => FinishReason::ContentFilter,
                                            _ => FinishReason::Stop,
                                        });
                                    yield Ok(TokenDelta {
                                        text: choice.delta.content,
                                        tool_call: None,
                                        finish_reason,
                                    });
                                }
                            }
                            Err(e) => {
                                tracing::debug!("skipping malformed SSE chunk: {e}: {data}");
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Locate the end of a complete SSE event in a string buffer.
/// SSE events are separated by blank lines (`\n\n` or `\r\n\r\n`).
pub(crate) fn find_event_end(s: &str) -> Option<usize> {
    s.find("\n\n")
        .map(|i| i + 2)
        .or_else(|| s.find("\r\n\r\n").map(|i| i + 4))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_config_constructor() {
        let cfg = OpenAiConfig::openai("sk-test-key");
        assert_eq!(cfg.provider_id.0, "openai");
        assert_eq!(cfg.base_url, "https://api.openai.com/v1");
        assert_eq!(cfg.api_key.as_deref(), Some("sk-test-key"));
        assert!(cfg.supports_embeddings);
        assert!(cfg.supports_model_discovery);
        assert_eq!(cfg.timeout_secs, 120);
    }

    #[test]
    fn openai_config_default_has_no_key() {
        let cfg = OpenAiConfig::default();
        assert!(cfg.api_key.is_none());
    }

    #[test]
    fn openai_adapter_capabilities() {
        let cfg = OpenAiConfig::openai("sk-test");
        let adapter = OpenAiAdapter::new(cfg).unwrap();
        let caps = adapter.capabilities(&ModelId::new("gpt-4o"));
        assert!(caps.streaming);
        assert!(caps.tool_calling);
        assert!(caps.embeddings);
        assert!(caps.supports_api_key);
        assert!(caps.supports_model_discovery);
        assert!(!caps.supports_oauth_browser);
    }

    #[test]
    fn openai_adapter_capabilities_no_embeddings() {
        let cfg = OpenAiConfig {
            supports_embeddings: false,
            ..OpenAiConfig::openai("sk-test")
        };
        let adapter = OpenAiAdapter::new(cfg).unwrap();
        let caps = adapter.capabilities(&ModelId::new("gpt-4o"));
        assert!(!caps.embeddings);
    }

    #[test]
    fn find_event_end_double_newline() {
        assert_eq!(find_event_end("data: hello\n\ndata: world"), Some(13));
    }

    #[test]
    fn find_event_end_crlf() {
        assert_eq!(find_event_end("data: hello\r\n\r\ndata: world"), Some(15));
    }

    #[test]
    fn find_event_end_incomplete() {
        assert_eq!(find_event_end("data: hello\n"), None);
    }

    #[test]
    fn find_event_end_empty() {
        assert_eq!(find_event_end(""), None);
    }

    #[test]
    fn convert_message_to_wire_text() {
        let msg = ChatMessage {
            role: Role::User,
            content: MessageContent::Text("hello".to_string()),
        };
        let wire = OpenAiAdapter::convert_message_to_wire(&msg);
        assert_eq!(wire.role, "user");
        assert_eq!(wire.content, serde_json::Value::String("hello".to_string()));
        assert!(wire.tool_call_id.is_none());
    }

    #[test]
    fn convert_wire_to_message_assistant() {
        let wire = WireChatMessage {
            role: "assistant".to_string(),
            content: serde_json::Value::String("world".to_string()),
            tool_calls: None,
            tool_call_id: None,
        };
        let msg = OpenAiAdapter::convert_wire_to_message(&wire);
        assert!(matches!(msg.role, Role::Assistant));
        if let MessageContent::Text(t) = msg.content {
            assert_eq!(t, "world");
        } else {
            panic!("expected Text");
        }
    }

    #[test]
    fn convert_finish_reason_variants() {
        assert!(matches!(
            OpenAiAdapter::convert_finish_reason(Some("stop")),
            FinishReason::Stop
        ));
        assert!(matches!(
            OpenAiAdapter::convert_finish_reason(Some("length")),
            FinishReason::Length
        ));
        assert!(matches!(
            OpenAiAdapter::convert_finish_reason(Some("tool_calls")),
            FinishReason::ToolCalls
        ));
        assert!(matches!(
            OpenAiAdapter::convert_finish_reason(Some("content_filter")),
            FinishReason::ContentFilter
        ));
        assert!(matches!(
            OpenAiAdapter::convert_finish_reason(None),
            FinishReason::Stop
        ));
    }

    #[tokio::test]
    async fn auth_state_with_key_is_authenticated() {
        let cfg = OpenAiConfig::openai("sk-test");
        let adapter = OpenAiAdapter::new(cfg).unwrap();
        let state = adapter.auth_state().await;
        assert!(matches!(state, AuthState::Authenticated { .. }));
    }

    #[tokio::test]
    async fn auth_state_without_key_is_unauthenticated() {
        let cfg = OpenAiConfig::default();
        let adapter = OpenAiAdapter::new(cfg).unwrap();
        let state = adapter.auth_state().await;
        assert!(matches!(state, AuthState::Unauthenticated));
    }
}
