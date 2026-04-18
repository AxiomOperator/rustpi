//! Provider abstraction layer.
//!
//! All model providers implement [`ModelProvider`]. Adapters normalize
//! provider-specific errors into [`ProviderError`] before crossing crate boundaries.
//!
//! # Implemented adapters (Phase 4+)
//! - OpenAI-compatible (including local endpoints)
//! - llama.cpp
//! - vLLM  
//! - GitHub Copilot (OAuth)
//! - Gemini (OAuth / API key)
//!
//! Phase 0 stub — trait defined, implementations deferred to Phase 4.

pub mod capability;
pub mod error;
pub mod provider;
pub mod registry;

pub use agent_core::types::{ModelId, ProviderId, ToolCall, ToolResult};
pub use capability::ProviderCapabilities;
pub use error::ProviderError;
pub use provider::{
    ChatMessage, CompletionRequest, CompletionResponse, EmbeddingRequest, EmbeddingResponse,
    FinishReason, MessageContent, ModelInfo, ModelProvider, ProviderMetadata, Role, TokenDelta,
    TokenUsage,
};
pub use registry::ProviderRegistry;
