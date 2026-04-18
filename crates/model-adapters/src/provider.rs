//! Core provider trait.

use crate::{ProviderCapabilities, ProviderError};
use agent_core::types::{AuthState, ModelId, ProviderId, ToolCall};
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

    /// List available models.
    async fn list_models(&self) -> Result<Vec<ModelId>, ProviderError>;

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

    /// Generate embeddings for a list of inputs.
    async fn embed(&self, inputs: Vec<String>) -> Result<Vec<Vec<f32>>, ProviderError>;

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
