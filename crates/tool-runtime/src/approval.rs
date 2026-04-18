//! Approval hooks for sensitive tool executions.
//!
//! Before a tool marked `High` or `Critical` sensitivity executes, the runtime
//! checks the registered [`ApprovalHook`]. This allows the application layer
//! to implement interactive prompts, pre-approved lists, or policy-driven gates.

use crate::schema::{ToolSensitivity};
use async_trait::async_trait;

/// The result of an approval check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalDecision {
    /// Proceed with execution.
    Approved,
    /// Block execution — execution will fail with `ToolError::PolicyDenied`.
    Denied(String),
    /// Execution requires interactive approval that is not yet resolved.
    /// The runtime treats this as Denied.
    Pending(String),
}

/// Context passed to an approval hook.
#[derive(Debug, Clone)]
pub struct ApprovalContext {
    pub tool_name: String,
    pub sensitivity: ToolSensitivity,
    pub args: serde_json::Value,
}

/// Hook called before any tool with `High` or `Critical` sensitivity executes.
#[async_trait]
pub trait ApprovalHook: Send + Sync {
    async fn check(&self, ctx: &ApprovalContext) -> ApprovalDecision;
}

/// Auto-approves all requests. Suitable for testing and fully-trusted environments.
pub struct AutoApprove;

#[async_trait]
impl ApprovalHook for AutoApprove {
    async fn check(&self, _ctx: &ApprovalContext) -> ApprovalDecision {
        ApprovalDecision::Approved
    }
}

/// Auto-denies all requests at or above a given sensitivity threshold.
pub struct DenyAbove {
    pub threshold: ToolSensitivity,
}

#[async_trait]
impl ApprovalHook for DenyAbove {
    async fn check(&self, ctx: &ApprovalContext) -> ApprovalDecision {
        if ctx.sensitivity >= self.threshold {
            ApprovalDecision::Denied(format!(
                "tool '{}' requires approval (sensitivity: {:?})",
                ctx.tool_name, ctx.sensitivity
            ))
        } else {
            ApprovalDecision::Approved
        }
    }
}

/// Approves only tools in a pre-configured allow-list.
pub struct AllowList {
    allowed: std::collections::HashSet<String>,
}

impl AllowList {
    pub fn new(tools: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            allowed: tools.into_iter().map(Into::into).collect(),
        }
    }
}

#[async_trait]
impl ApprovalHook for AllowList {
    async fn check(&self, ctx: &ApprovalContext) -> ApprovalDecision {
        if self.allowed.contains(&ctx.tool_name) {
            ApprovalDecision::Approved
        } else {
            ApprovalDecision::Denied(format!(
                "tool '{}' is not in the approval allow-list",
                ctx.tool_name
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn auto_approve_approves_everything() {
        let hook = AutoApprove;
        let ctx = ApprovalContext {
            tool_name: "shell".to_string(),
            sensitivity: ToolSensitivity::Critical,
            args: serde_json::json!({}),
        };
        assert_eq!(hook.check(&ctx).await, ApprovalDecision::Approved);
    }

    #[tokio::test]
    async fn deny_above_blocks_high_sensitivity() {
        let hook = DenyAbove {
            threshold: ToolSensitivity::High,
        };
        let ctx = ApprovalContext {
            tool_name: "shell".to_string(),
            sensitivity: ToolSensitivity::High,
            args: serde_json::json!({}),
        };
        assert!(matches!(hook.check(&ctx).await, ApprovalDecision::Denied(_)));
    }

    #[tokio::test]
    async fn deny_above_allows_safe() {
        let hook = DenyAbove {
            threshold: ToolSensitivity::High,
        };
        let ctx = ApprovalContext {
            tool_name: "read_file".to_string(),
            sensitivity: ToolSensitivity::Safe,
            args: serde_json::json!({}),
        };
        assert_eq!(hook.check(&ctx).await, ApprovalDecision::Approved);
    }

    #[tokio::test]
    async fn allow_list_denies_unlisted() {
        let hook = AllowList::new(["read_file"]);
        let ctx = ApprovalContext {
            tool_name: "shell".to_string(),
            sensitivity: ToolSensitivity::Safe,
            args: serde_json::json!({}),
        };
        assert!(matches!(hook.check(&ctx).await, ApprovalDecision::Denied(_)));
    }
}
