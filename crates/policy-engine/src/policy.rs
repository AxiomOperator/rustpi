//! Policy types and glob-based rule evaluation.

use glob::Pattern;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::decision::PolicyDecision;
use crate::request::{AuthAction, AuthRequest, FileMutationRequest, ProviderRequest, ToolRequest};

/// The verdict from a policy evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyVerdict {
    Allow,
    Deny { reason: String },
    /// Require explicit user approval before proceeding.
    RequireApproval { prompt: String },
}

/// A single policy rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRule {
    pub name: String,
    pub target: PolicyTarget,
    pub action: PolicyAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyTarget {
    /// Matches a tool by name glob (e.g. "shell_*").
    Tool { name_glob: String },
    /// Matches a file path by glob.
    FilePath { path_glob: String },
    /// Matches a provider by id (glob supported).
    Provider { id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyAction {
    Allow,
    Deny,
    RequireApproval,
}

/// Default policy applied when no rule matches.
#[derive(Debug, Clone, Default)]
pub enum DefaultPolicy {
    #[default]
    Allow,
    Deny,
}

/// Policy evaluator with first-match-wins glob rule evaluation.
#[derive(Default)]
pub struct PolicyEngine {
    rules: Vec<PolicyRule>,
    default_tool_policy: DefaultPolicy,
}

impl PolicyEngine {
    pub fn new(rules: Vec<PolicyRule>) -> Self {
        Self {
            rules,
            default_tool_policy: DefaultPolicy::Allow,
        }
    }

    pub fn with_default_policy(mut self, policy: DefaultPolicy) -> Self {
        self.default_tool_policy = policy;
        self
    }

    /// Evaluate whether a tool may execute.
    ///
    /// Rules are evaluated in order; first match wins. Falls back to `default_tool_policy`.
    pub fn evaluate_tool(&self, req: &ToolRequest) -> PolicyDecision {
        for rule in &self.rules {
            if let PolicyTarget::Tool { name_glob } = &rule.target {
                if glob_matches(name_glob, &req.tool_name) {
                    debug!(rule = %rule.name, tool = %req.tool_name, "tool rule matched");
                    return apply_action(&rule.name, &rule.action, &req.tool_name);
                }
            }
        }
        match self.default_tool_policy {
            DefaultPolicy::Allow => PolicyDecision::allow(format!(
                "tool '{}' allowed by default policy",
                req.tool_name
            )),
            DefaultPolicy::Deny => PolicyDecision::deny(
                "<default>",
                format!("tool '{}' denied by default policy", req.tool_name),
            ),
        }
    }

    /// Evaluate whether a file mutation is permitted.
    pub fn evaluate_file_mutation(&self, req: &FileMutationRequest) -> PolicyDecision {
        let path_str = req.path.to_string_lossy();
        for rule in &self.rules {
            if let PolicyTarget::FilePath { path_glob } = &rule.target {
                if glob_matches(path_glob, &path_str) {
                    debug!(rule = %rule.name, path = %path_str, "file-path rule matched");
                    return apply_action(&rule.name, &rule.action, &path_str);
                }
            }
        }
        PolicyDecision::allow(format!("path '{}' allowed by default policy", path_str))
    }

    /// Evaluate whether a provider may be used.
    pub fn evaluate_provider(&self, req: &ProviderRequest) -> PolicyDecision {
        for rule in &self.rules {
            if let PolicyTarget::Provider { id } = &rule.target {
                if glob_matches(id, &req.provider_id) {
                    debug!(rule = %rule.name, provider = %req.provider_id, "provider rule matched");
                    return apply_action(&rule.name, &rule.action, &req.provider_id);
                }
            }
        }
        PolicyDecision::allow(format!(
            "provider '{}' allowed by default policy",
            req.provider_id
        ))
    }

    /// Evaluate whether an auth action is permitted.
    ///
    /// Guardrail: `UseToken` always requires `is_authenticated == true`.
    pub fn evaluate_auth(&self, req: &AuthRequest) -> PolicyDecision {
        if req.action == AuthAction::UseToken && !req.is_authenticated {
            return PolicyDecision::deny(
                "<guardrail>",
                format!("provider '{}' is not authenticated", req.provider_id),
            );
        }
        for rule in &self.rules {
            if let PolicyTarget::Provider { id } = &rule.target {
                if glob_matches(id, &req.provider_id) {
                    debug!(rule = %rule.name, provider = %req.provider_id, "auth rule matched");
                    return apply_action(&rule.name, &rule.action, &req.provider_id);
                }
            }
        }
        PolicyDecision::allow(format!(
            "auth action for provider '{}' allowed by default policy",
            req.provider_id
        ))
    }
}

fn glob_matches(pattern: &str, value: &str) -> bool {
    match Pattern::new(pattern) {
        Ok(p) => p.matches(value),
        Err(_) => {
            tracing::warn!(pattern, "invalid glob pattern, treating as no-match");
            false
        }
    }
}

fn apply_action(rule_name: &str, action: &PolicyAction, subject: &str) -> PolicyDecision {
    match action {
        PolicyAction::Allow => PolicyDecision {
            verdict: PolicyVerdict::Allow,
            matched_rule: Some(rule_name.to_string()),
            reason: format!("'{}' allowed by rule '{}'", subject, rule_name),
        },
        PolicyAction::Deny => PolicyDecision::deny(
            rule_name,
            format!("'{}' denied by rule '{}'", subject, rule_name),
        ),
        PolicyAction::RequireApproval => PolicyDecision::require_approval(
            rule_name,
            format!("'{}' requires approval per rule '{}'", subject, rule_name),
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::request::{
        AuthAction, AuthRequest, FileOperation, FileMutationRequest, ProviderOperation,
        ProviderRequest, ToolRequest,
    };

    fn tool_req(name: &str) -> ToolRequest {
        ToolRequest {
            tool_name: name.to_string(),
            args: serde_json::Value::Null,
        }
    }

    fn file_req(path: &str) -> FileMutationRequest {
        FileMutationRequest {
            path: PathBuf::from(path),
            operation: FileOperation::Write,
        }
    }

    fn provider_req(id: &str) -> ProviderRequest {
        ProviderRequest {
            provider_id: id.to_string(),
            model_id: None,
            operation: ProviderOperation::Chat,
        }
    }

    fn auth_req(provider_id: &str, action: AuthAction, is_authenticated: bool) -> AuthRequest {
        AuthRequest {
            provider_id: provider_id.to_string(),
            action,
            is_authenticated,
        }
    }

    fn rule(name: &str, target: PolicyTarget, action: PolicyAction) -> PolicyRule {
        PolicyRule {
            name: name.to_string(),
            target,
            action,
        }
    }

    // 1. Tool allow by glob
    #[test]
    fn test_tool_allow_by_glob() {
        let engine = PolicyEngine::new(vec![rule(
            "allow-read",
            PolicyTarget::Tool { name_glob: "read_*".to_string() },
            PolicyAction::Allow,
        )]);
        let d = engine.evaluate_tool(&tool_req("read_file"));
        assert!(d.is_allowed());
        assert_eq!(d.matched_rule.as_deref(), Some("allow-read"));
    }

    // 2. Tool deny by exact name
    #[test]
    fn test_tool_deny_exact() {
        let engine = PolicyEngine::new(vec![rule(
            "deny-rm",
            PolicyTarget::Tool { name_glob: "rm".to_string() },
            PolicyAction::Deny,
        )]);
        let d = engine.evaluate_tool(&tool_req("rm"));
        assert!(d.is_denied());
        assert_eq!(d.matched_rule.as_deref(), Some("deny-rm"));
    }

    // 3. Tool deny by glob
    #[test]
    fn test_tool_deny_by_glob() {
        let engine = PolicyEngine::new(vec![rule(
            "deny-shell",
            PolicyTarget::Tool { name_glob: "shell_*".to_string() },
            PolicyAction::Deny,
        )]);
        let d = engine.evaluate_tool(&tool_req("shell_exec"));
        assert!(d.is_denied());
    }

    // 4. First-match-wins: deny rule before allow rule → Deny
    #[test]
    fn test_first_match_wins_deny_before_allow() {
        let engine = PolicyEngine::new(vec![
            rule(
                "deny-shell",
                PolicyTarget::Tool { name_glob: "shell_*".to_string() },
                PolicyAction::Deny,
            ),
            rule(
                "allow-all",
                PolicyTarget::Tool { name_glob: "*".to_string() },
                PolicyAction::Allow,
            ),
        ]);
        let d = engine.evaluate_tool(&tool_req("shell_exec"));
        assert!(d.is_denied());
        assert_eq!(d.matched_rule.as_deref(), Some("deny-shell"));
    }

    // 5. No matching rule → default Allow
    #[test]
    fn test_no_match_default_allow() {
        let engine = PolicyEngine::new(vec![]);
        let d = engine.evaluate_tool(&tool_req("unknown_tool"));
        assert!(d.is_allowed());
        assert!(d.matched_rule.is_none());
    }

    // 6. No matching rule with DefaultPolicy::Deny → Deny
    #[test]
    fn test_no_match_default_deny() {
        let engine =
            PolicyEngine::new(vec![]).with_default_policy(DefaultPolicy::Deny);
        let d = engine.evaluate_tool(&tool_req("anything"));
        assert!(d.is_denied());
        assert_eq!(d.matched_rule.as_deref(), Some("<default>"));
    }

    // 7. File mutation denied outside allowed glob
    #[test]
    fn test_file_denied_outside_glob() {
        let engine = PolicyEngine::new(vec![rule(
            "deny-etc",
            PolicyTarget::FilePath { path_glob: "/etc/*".to_string() },
            PolicyAction::Deny,
        )]);
        let d = engine.evaluate_file_mutation(&file_req("/etc/passwd"));
        assert!(d.is_denied());
    }

    // 8. File mutation allowed inside allowed glob
    #[test]
    fn test_file_allowed_inside_glob() {
        let engine = PolicyEngine::new(vec![rule(
            "allow-workspace",
            PolicyTarget::FilePath { path_glob: "/workspace/**".to_string() },
            PolicyAction::Allow,
        )]);
        let d = engine.evaluate_file_mutation(&file_req("/workspace/src/main.rs"));
        assert!(d.is_allowed());
        assert_eq!(d.matched_rule.as_deref(), Some("allow-workspace"));
    }

    // 9. Provider denied by name
    #[test]
    fn test_provider_denied_by_name() {
        let engine = PolicyEngine::new(vec![rule(
            "deny-openai",
            PolicyTarget::Provider { id: "openai".to_string() },
            PolicyAction::Deny,
        )]);
        let d = engine.evaluate_provider(&provider_req("openai"));
        assert!(d.is_denied());
    }

    // 10. Auth UseToken when not authenticated → always Deny
    #[test]
    fn test_auth_use_token_not_authenticated() {
        let engine = PolicyEngine::new(vec![rule(
            "allow-all-providers",
            PolicyTarget::Provider { id: "*".to_string() },
            PolicyAction::Allow,
        )]);
        let d = engine.evaluate_auth(&auth_req("openai", AuthAction::UseToken, false));
        assert!(d.is_denied());
        assert_eq!(d.matched_rule.as_deref(), Some("<guardrail>"));
    }

    // 11. Auth UseToken when authenticated → Allow (no blocking rule)
    #[test]
    fn test_auth_use_token_authenticated() {
        let engine = PolicyEngine::new(vec![]);
        let d = engine.evaluate_auth(&auth_req("openai", AuthAction::UseToken, true));
        assert!(d.is_allowed());
    }

    // 12. RequireApproval verdict
    #[test]
    fn test_require_approval_verdict() {
        let engine = PolicyEngine::new(vec![rule(
            "approve-deploy",
            PolicyTarget::Tool { name_glob: "deploy_*".to_string() },
            PolicyAction::RequireApproval,
        )]);
        let d = engine.evaluate_tool(&tool_req("deploy_prod"));
        assert!(!d.is_allowed());
        assert!(!d.is_denied());
        assert!(matches!(d.verdict, PolicyVerdict::RequireApproval { .. }));
        assert_eq!(d.matched_rule.as_deref(), Some("approve-deploy"));
    }

    // 13. PolicyDecision::is_allowed and is_denied helpers
    #[test]
    fn test_decision_helpers() {
        let allow = PolicyDecision::allow("ok");
        assert!(allow.is_allowed());
        assert!(!allow.is_denied());

        let deny = PolicyDecision::deny("rule", "blocked");
        assert!(!deny.is_allowed());
        assert!(deny.is_denied());

        let approval = PolicyDecision::require_approval("rule", "needs approval");
        assert!(!approval.is_allowed());
        assert!(!approval.is_denied());
    }

    // 14. Tool glob does not match unrelated names
    #[test]
    fn test_tool_glob_no_match() {
        let engine = PolicyEngine::new(vec![rule(
            "deny-shell",
            PolicyTarget::Tool { name_glob: "shell_*".to_string() },
            PolicyAction::Deny,
        )]);
        let d = engine.evaluate_tool(&tool_req("read_file"));
        assert!(d.is_allowed()); // falls through to default allow
    }
}

