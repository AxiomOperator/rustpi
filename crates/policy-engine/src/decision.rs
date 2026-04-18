//! Structured policy decision returned by the engine.

use serde::{Deserialize, Serialize};

use crate::policy::PolicyVerdict;

/// A structured policy decision returned by the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyDecision {
    pub verdict: PolicyVerdict,
    /// Name of the rule that matched, if any.
    pub matched_rule: Option<String>,
    /// Human-readable reason for the decision.
    pub reason: String,
}

impl PolicyDecision {
    pub fn allow(reason: impl Into<String>) -> Self {
        Self {
            verdict: PolicyVerdict::Allow,
            matched_rule: None,
            reason: reason.into(),
        }
    }

    pub fn deny(rule: impl Into<String>, reason: impl Into<String>) -> Self {
        let rule = rule.into();
        let reason = reason.into();
        Self {
            verdict: PolicyVerdict::Deny { reason: reason.clone() },
            matched_rule: Some(rule),
            reason,
        }
    }

    pub fn require_approval(rule: impl Into<String>, prompt: impl Into<String>) -> Self {
        let rule = rule.into();
        let prompt = prompt.into();
        Self {
            verdict: PolicyVerdict::RequireApproval { prompt: prompt.clone() },
            matched_rule: Some(rule),
            reason: prompt,
        }
    }

    pub fn is_allowed(&self) -> bool {
        matches!(self.verdict, PolicyVerdict::Allow)
    }

    pub fn is_denied(&self) -> bool {
        matches!(self.verdict, PolicyVerdict::Deny { .. })
    }
}
