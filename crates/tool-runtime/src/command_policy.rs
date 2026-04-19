//! Command allow/deny policy for subprocess execution.
//!
//! Evaluated BEFORE spawning any subprocess. First-match-wins across rules.
//! Built-in deny patterns protect against the most dangerous commands.

use crate::ToolError;

/// Decision from command policy evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandDecision {
    Allow,
    Deny { reason: String },
}

/// A single command policy rule.
#[derive(Debug, Clone)]
pub struct CommandRule {
    pub name: &'static str,
    pub pattern: CommandPattern,
    pub action: CommandAction,
}

#[derive(Debug, Clone)]
pub enum CommandPattern {
    /// Command string contains this substring (case-insensitive).
    Contains(String),
    /// Command string starts with this prefix (after trimming whitespace).
    StartsWith(String),
    /// Command exactly matches this string.
    Exact(String),
}

#[derive(Debug, Clone)]
pub enum CommandAction {
    Allow,
    Deny,
}

/// Policy that evaluates shell command strings before execution.
pub struct CommandPolicy {
    rules: Vec<CommandRule>,
}

impl CommandPolicy {
    /// Create a policy with only the provided rules (no built-in denies).
    pub fn new(rules: Vec<CommandRule>) -> Self {
        Self { rules }
    }

    /// Create a policy with the default set of dangerous-command deny rules.
    pub fn with_defaults() -> Self {
        let rules = vec![
            CommandRule {
                name: "rm with root path",
                pattern: CommandPattern::Contains("rm -rf /".into()),
                action: CommandAction::Deny,
            },
            CommandRule {
                name: "rm with root wildcard",
                pattern: CommandPattern::Contains("rm -rf /*".into()),
                action: CommandAction::Deny,
            },
            CommandRule {
                name: "dd disk overwrite",
                pattern: CommandPattern::Contains("dd if=/dev/".into()),
                action: CommandAction::Deny,
            },
            CommandRule {
                name: "mkfs filesystem format",
                pattern: CommandPattern::StartsWith("mkfs".into()),
                action: CommandAction::Deny,
            },
            CommandRule {
                name: "fork bomb",
                pattern: CommandPattern::Contains(":()".into()),
                action: CommandAction::Deny,
            },
            CommandRule {
                name: "chmod recursive world-write on root",
                pattern: CommandPattern::Contains("chmod -R 777 /".into()),
                action: CommandAction::Deny,
            },
            CommandRule {
                name: "chmod world-write on root",
                pattern: CommandPattern::Contains("chmod 777 /".into()),
                action: CommandAction::Deny,
            },
            CommandRule {
                name: "direct disk write",
                pattern: CommandPattern::Contains("> /dev/sda".into()),
                action: CommandAction::Deny,
            },
            CommandRule {
                name: "sda device access",
                pattern: CommandPattern::Contains("/dev/sda".into()),
                action: CommandAction::Deny,
            },
            CommandRule {
                name: "shred dev",
                pattern: CommandPattern::Contains("shred /dev/".into()),
                action: CommandAction::Deny,
            },
        ];
        Self { rules }
    }

    /// Evaluate a command string. Returns the first matching decision, or Allow if no match.
    pub fn evaluate(&self, command: &str) -> CommandDecision {
        let lower = command.to_lowercase();
        let trimmed = command.trim();

        for rule in &self.rules {
            let matched = match &rule.pattern {
                CommandPattern::Contains(s) => lower.contains(s.to_lowercase().as_str()),
                CommandPattern::StartsWith(s) => {
                    trimmed.to_lowercase().starts_with(s.to_lowercase().as_str())
                }
                CommandPattern::Exact(s) => trimmed == s.as_str(),
            };

            if matched {
                return match rule.action {
                    CommandAction::Allow => CommandDecision::Allow,
                    CommandAction::Deny => CommandDecision::Deny {
                        reason: format!("rule '{}' denied command", rule.name),
                    },
                };
            }
        }

        CommandDecision::Allow
    }

    /// Evaluate and return a ToolError if denied.
    pub fn check(&self, command: &str) -> Result<(), ToolError> {
        match self.evaluate(command) {
            CommandDecision::Allow => Ok(()),
            CommandDecision::Deny { reason } => Err(ToolError::CommandDenied(reason)),
        }
    }

    pub fn rules(&self) -> &[CommandRule] {
        &self.rules
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allow_safe_command() {
        let policy = CommandPolicy::with_defaults();
        assert_eq!(policy.evaluate("echo hello"), CommandDecision::Allow);
    }

    #[test]
    fn deny_rm_rf_slash() {
        let policy = CommandPolicy::with_defaults();
        assert!(matches!(policy.evaluate("rm -rf /"), CommandDecision::Deny { .. }));
    }

    #[test]
    fn deny_rm_rf_wildcard() {
        let policy = CommandPolicy::with_defaults();
        assert!(matches!(policy.evaluate("rm -rf /*"), CommandDecision::Deny { .. }));
    }

    #[test]
    fn deny_dd_dev() {
        let policy = CommandPolicy::with_defaults();
        assert!(matches!(
            policy.evaluate("dd if=/dev/sda of=/dev/sdb"),
            CommandDecision::Deny { .. }
        ));
    }

    #[test]
    fn deny_mkfs() {
        let policy = CommandPolicy::with_defaults();
        assert!(matches!(policy.evaluate("mkfs.ext4 /dev/sdb"), CommandDecision::Deny { .. }));
    }

    #[test]
    fn deny_fork_bomb() {
        let policy = CommandPolicy::with_defaults();
        assert!(matches!(
            policy.evaluate(":(){ :|:& };:"),
            CommandDecision::Deny { .. }
        ));
    }

    #[test]
    fn deny_chmod_root() {
        let policy = CommandPolicy::with_defaults();
        assert!(matches!(
            policy.evaluate("chmod -R 777 /"),
            CommandDecision::Deny { .. }
        ));
    }

    #[test]
    fn custom_deny_rule() {
        let policy = CommandPolicy::new(vec![CommandRule {
            name: "no curl",
            pattern: CommandPattern::Contains("curl".into()),
            action: CommandAction::Deny,
        }]);
        assert!(matches!(
            policy.evaluate("curl https://example.com"),
            CommandDecision::Deny { .. }
        ));
    }

    #[test]
    fn first_match_wins_allow_before_deny() {
        let policy = CommandPolicy::new(vec![
            CommandRule {
                name: "allow curl example.com",
                pattern: CommandPattern::Contains("curl example.com".into()),
                action: CommandAction::Allow,
            },
            CommandRule {
                name: "deny curl",
                pattern: CommandPattern::Contains("curl".into()),
                action: CommandAction::Deny,
            },
        ]);
        assert_eq!(
            policy.evaluate("curl example.com"),
            CommandDecision::Allow
        );
    }

    #[test]
    fn check_returns_err_for_denied() {
        let policy = CommandPolicy::with_defaults();
        let result = policy.check("rm -rf /");
        assert!(matches!(result, Err(ToolError::CommandDenied(_))));
    }

    #[tokio::test]
    async fn shell_tool_rejects_rm_rf() {
        use crate::tools::shell::ShellTool;
        use crate::registry::Tool;
        use agent_core::types::ToolCall;
        use serde_json::json;

        let tool = ShellTool::new();
        let call = ToolCall {
            id: "deny-test".into(),
            name: "shell".into(),
            arguments: json!({"command": "rm -rf /"}),
        };
        let result = tool.execute(call).await;
        assert!(matches!(result, Err(ToolError::CommandDenied(_))));
    }

    #[test]
    fn deny_command_with_leading_whitespace() {
        let policy = CommandPolicy::with_defaults();
        // Leading whitespace must not prevent detection of "rm -rf /" via Contains
        assert!(
            matches!(policy.evaluate("  rm -rf /"), CommandDecision::Deny { .. }),
            "leading whitespace must not bypass Contains-based deny rule"
        );
    }

    #[test]
    fn deny_dev_sda_write() {
        let policy = CommandPolicy::with_defaults();
        assert!(
            matches!(policy.evaluate("cat file > /dev/sda"), CommandDecision::Deny { .. }),
            "writing to /dev/sda must be denied"
        );
    }

    #[test]
    fn allow_curl_by_default() {
        let policy = CommandPolicy::with_defaults();
        assert_eq!(
            policy.evaluate("curl https://example.com"),
            CommandDecision::Allow,
            "curl must be allowed by the default policy"
        );
    }

    #[test]
    fn allow_git_commands() {
        let policy = CommandPolicy::with_defaults();
        assert_eq!(policy.evaluate("git status"), CommandDecision::Allow);
        assert_eq!(policy.evaluate("git commit -m 'test'"), CommandDecision::Allow);
        assert_eq!(policy.evaluate("git push origin main"), CommandDecision::Allow);
    }
}
