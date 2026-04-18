//! Tool metadata schema: describes a tool's identity, parameters, sensitivity, and runtime config.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Sensitivity/risk level of a tool.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSensitivity {
    /// Safe to run automatically. No approval needed.
    Safe,
    /// Low-risk but should be logged prominently.
    Low,
    /// Requires approval before execution (e.g. file writes outside sandbox).
    High,
    /// Always requires explicit approval; never auto-approved.
    Critical,
}

impl Default for ToolSensitivity {
    fn default() -> Self {
        ToolSensitivity::Safe
    }
}

/// Runtime configuration for a tool invocation.
#[derive(Debug, Clone, Default)]
pub struct ToolConfig {
    /// Maximum execution time. None means use the runner's default.
    pub timeout: Option<Duration>,
    /// Whether this specific invocation requires approval regardless of tool sensitivity.
    pub require_approval: bool,
    /// RunId context for event emission.
    pub run_id: Option<agent_core::types::RunId>,
    /// Cancellation token for this invocation.
    pub cancel: Option<tokio_util::sync::CancellationToken>,
}

/// Static schema describing a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    /// Unique tool name (used in ToolCall.name).
    pub name: String,
    /// Human-readable description shown to the model.
    pub description: String,
    /// JSON Schema for the tool's parameters.
    pub parameters: serde_json::Value,
    /// Sensitivity/risk level.
    pub sensitivity: ToolSensitivity,
    /// Default timeout in seconds. None = use runner default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_timeout_secs: Option<u64>,
}

impl ToolSchema {
    pub fn default_timeout(&self) -> Option<Duration> {
        self.default_timeout_secs.map(Duration::from_secs)
    }
}
