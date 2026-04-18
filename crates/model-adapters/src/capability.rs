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
}
