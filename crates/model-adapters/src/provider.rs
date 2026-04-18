//! Core provider trait.

use crate::{ProviderCapabilities, ProviderError};
use agent_core::types::{AuthFlow, AuthState, ModelId, ProviderId, ToolCall};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// A single message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: MessageContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    ToolResult { call_id: String, output: serde_json::Value },
}

/// A streaming token delta from the model.
#[derive(Debug, Clone)]
pub struct TokenDelta {
    pub text: Option<String>,
    pub tool_call: Option<ToolCall>,
    pub finish_reason: Option<FinishReason>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    Length,
    ToolCalls,
    ContentFilter,
}

/// Chat completion request parameters.
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub model: ModelId,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    /// Tool schemas available to the model, as JSON Schema objects.
    pub tools: Vec<serde_json::Value>,
}

/// The unified provider interface all adapters must implement.
#[async_trait]
pub trait ModelProvider: Send + Sync {
    fn provider_id(&self) -> &ProviderId;
    fn capabilities(&self, model: &ModelId) -> ProviderCapabilities;

    /// Static provider metadata (name, description, supported auth flows).
    fn metadata(&self) -> ProviderMetadata;

    /// List available models with their metadata.
    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError>;

    /// Non-streaming chat completion.
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError>;

    /// Streaming chat completion. Returns a stream of deltas.
    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<
        std::pin::Pin<Box<dyn futures::Stream<Item = Result<TokenDelta, ProviderError>> + Send>>,
        ProviderError,
    >;

    /// Generate embeddings for the given inputs.
    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse, ProviderError>;

    /// Current auth state for this provider.
    async fn auth_state(&self) -> AuthState;
}

/// A non-streaming completion response.
#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub message: ChatMessage,
    pub finish_reason: FinishReason,
    pub usage: TokenUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Embedding generation request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    pub model: ModelId,
    pub inputs: Vec<String>,
    /// Optional dimensions hint (for models that support it).
    pub dimensions: Option<u32>,
}

/// Embedding generation response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    pub embeddings: Vec<Vec<f32>>,
    pub model: ModelId,
    pub usage: TokenUsage,
}

/// Metadata about a specific model offered by a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: ModelId,
    pub display_name: Option<String>,
    /// Maximum context window in tokens, if known.
    pub context_window: Option<u32>,
    /// Whether this model supports tool/function calling.
    pub supports_tools: bool,
    /// Whether this model supports vision/image inputs.
    pub supports_vision: bool,
    /// Whether this model supports embeddings.
    pub supports_embeddings: bool,
}

/// Static metadata about a provider (not per-model).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderMetadata {
    pub id: ProviderId,
    pub display_name: String,
    /// Human-readable description.
    pub description: String,
    /// Auth flows supported by this provider.
    pub supported_auth_flows: Vec<AuthFlow>,
    /// Whether this provider requires network access.
    pub requires_network: bool,
}
