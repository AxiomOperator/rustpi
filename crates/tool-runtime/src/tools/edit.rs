//! Targeted file edit tool (find-and-replace).
//!
//! Replaces the first (or all) occurrences of an exact string in a file.
//! This is intentionally deterministic — it does NOT do fuzzy matching.
//!
//! # Input schema
//! ```json
//! {
//!   "path": "/workspace/src/main.rs",
//!   "old_str": "let x = 1;",
//!   "new_str": "let x = 2;",
//!   "replace_all": false
//! }
//! ```
//!
//! # Error cases
//! - `old_str` not found in file → `InvalidArguments`
//! - Path outside allowed roots → `PathTraversal`

use crate::{overwrite_policy::OverwritePolicy, path_safety::PathSafetyPolicy, ToolError};
use agent_core::types::{ToolCall, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;

pub struct EditTool {
    policy: Arc<PathSafetyPolicy>,
    overwrite_policy: OverwritePolicy,
}

impl EditTool {
    pub fn new(policy: Arc<PathSafetyPolicy>) -> Self {
        Self {
            policy,
            overwrite_policy: OverwritePolicy::Allow,
        }
    }

    pub fn new_with_policy(
        policy: Arc<PathSafetyPolicy>,
        overwrite_policy: OverwritePolicy,
    ) -> Self {
        Self {
            policy,
            overwrite_policy,
        }
    }
}

#[async_trait]
impl crate::registry::Tool for EditTool {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn schema(&self) -> serde_json::Value {
        json!({
            "name": "edit_file",
            "description": "Replace a specific string in a file with a new string. Exact match only.",
            "x-sensitivity": "high",
            "parameters": {
                "type": "object",
                "required": ["path", "old_str", "new_str"],
                "properties": {
                    "path": { "type": "string" },
                    "old_str": { "type": "string", "description": "Exact string to find." },
                    "new_str": { "type": "string", "description": "Replacement string." },
                    "replace_all": { "type": "boolean", "default": false },
                    "overwrite_confirmed": {
                        "type": "boolean",
                        "description": "Set to true to confirm modifying the file when required by policy.",
                        "default": false
                    }
                }
            }
        })
    }

    async fn execute(&self, call: ToolCall) -> Result<ToolResult, ToolError> {
        let path_str = call
            .arguments
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments {
                tool: "edit_file".into(),
                reason: "missing 'path'".into(),
            })?;

        let old_str = call
            .arguments
            .get("old_str")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments {
                tool: "edit_file".into(),
                reason: "missing 'old_str'".into(),
            })?;

        let new_str = call
            .arguments
            .get("new_str")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments {
                tool: "edit_file".into(),
                reason: "missing 'new_str'".into(),
            })?;

        let replace_all = call
            .arguments
            .get("replace_all")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let overwrite_confirmed = call
            .arguments
            .get("overwrite_confirmed")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let safe_path = self.policy.validate(path_str)?;

        // Edit always targets an existing file; apply overwrite policy before I/O.
        // DenyExisting always denies; RequireConfirmation requires overwrite_confirmed=true.
        self.overwrite_policy.check(&safe_path, overwrite_confirmed)?;

        let original = tokio::fs::read_to_string(&safe_path)
            .await
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => ToolError::InvalidArguments {
                    tool: "edit_file".into(),
                    reason: format!("file not found: {}", safe_path.display()),
                },
                _ => ToolError::Io(e),
            })?;

        if !original.contains(old_str) {
            return Err(ToolError::InvalidArguments {
                tool: "edit_file".into(),
                reason: format!("old_str not found in {}", safe_path.display()),
            });
        }

        let occurrence_count = original.matches(old_str).count();
        let modified = if replace_all {
            original.replace(old_str, new_str)
        } else {
            original.replacen(old_str, new_str, 1)
        };

        tokio::fs::write(&safe_path, &modified)
            .await
            .map_err(ToolError::Io)?;

        let replacements = if replace_all { occurrence_count } else { 1 };
        Ok(ToolResult {
            call_id: call.id,
            success: true,
            output: json!({
                "path": safe_path.to_string_lossy(),
                "replacements": replacements,
                "replace_all": replace_all,
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Tool;
    use crate::OverwritePolicy;
    use tempfile::TempDir;

    fn policy_for(dir: &TempDir) -> Arc<PathSafetyPolicy> {
        Arc::new(PathSafetyPolicy::new([dir.path()]))
    }

    #[tokio::test]
    async fn edit_replaces_first_occurrence() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("file.txt");
        std::fs::write(&path, "hello world\nhello again\n").unwrap();

        let tool = EditTool::new(policy_for(&dir));
        let call = ToolCall {
            id: "e1".into(),
            name: "edit_file".into(),
            arguments: json!({
                "path": path.to_string_lossy(),
                "old_str": "hello",
                "new_str": "goodbye"
            }),
        };
        let result = tool.execute(call).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output["replacements"], 1);
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.starts_with("goodbye world"));
        assert!(content.contains("hello again")); // second not replaced
    }

    #[tokio::test]
    async fn edit_replaces_all_occurrences() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("file2.txt");
        std::fs::write(&path, "foo bar foo\n").unwrap();

        let tool = EditTool::new(policy_for(&dir));
        let call = ToolCall {
            id: "e2".into(),
            name: "edit_file".into(),
            arguments: json!({
                "path": path.to_string_lossy(),
                "old_str": "foo",
                "new_str": "baz",
                "replace_all": true
            }),
        };
        let result = tool.execute(call).await.unwrap();
        assert_eq!(result.output["replacements"], 2);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "baz bar baz\n");
    }

    #[tokio::test]
    async fn edit_errors_when_old_str_not_found() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("file3.txt");
        std::fs::write(&path, "some content\n").unwrap();

        let tool = EditTool::new(policy_for(&dir));
        let call = ToolCall {
            id: "e3".into(),
            name: "edit_file".into(),
            arguments: json!({
                "path": path.to_string_lossy(),
                "old_str": "not_present",
                "new_str": "replacement"
            }),
        };
        assert!(matches!(
            tool.execute(call).await,
            Err(ToolError::InvalidArguments { .. })
        ));
    }

    #[tokio::test]
    async fn edit_blocked_outside_root() {
        let dir = TempDir::new().unwrap();
        let tool = EditTool::new(policy_for(&dir));
        let call = ToolCall {
            id: "e4".into(),
            name: "edit_file".into(),
            arguments: json!({
                "path": "/etc/hosts",
                "old_str": "localhost",
                "new_str": "evil"
            }),
        };
        assert!(matches!(
            tool.execute(call).await,
            Err(ToolError::PathTraversal(_))
        ));
    }

    #[tokio::test]
    async fn edit_tool_deny_existing_always_denies() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("file.txt");
        std::fs::write(&path, "hello world\n").unwrap();
        let tool = EditTool::new_with_policy(policy_for(&dir), OverwritePolicy::DenyExisting);
        let call = ToolCall {
            id: "e5".into(),
            name: "edit_file".into(),
            arguments: json!({
                "path": path.to_string_lossy(),
                "old_str": "hello",
                "new_str": "goodbye"
            }),
        };
        assert!(matches!(
            tool.execute(call).await,
            Err(ToolError::OverwriteDenied(_))
        ));
    }

    #[tokio::test]
    async fn edit_tool_allow_policy_works_normally() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("file.txt");
        std::fs::write(&path, "hello world\n").unwrap();
        let tool = EditTool::new_with_policy(policy_for(&dir), OverwritePolicy::Allow);
        let call = ToolCall {
            id: "e6".into(),
            name: "edit_file".into(),
            arguments: json!({
                "path": path.to_string_lossy(),
                "old_str": "hello",
                "new_str": "goodbye"
            }),
        };
        let result = tool.execute(call).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output["replacements"], 1);
        assert!(std::fs::read_to_string(&path).unwrap().starts_with("goodbye"));
    }
}
