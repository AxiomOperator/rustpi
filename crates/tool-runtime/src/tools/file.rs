//! File read and write tools.
//!
//! Both tools route all path access through [`PathSafetyPolicy`] before any I/O.
//!
//! # ReadFileTool input schema
//! ```json
//! { "path": "/workspace/src/main.rs" }
//! ```
//!
//! # WriteFileTool input schema
//! ```json
//! {
//!   "path": "/workspace/src/main.rs",
//!   "content": "new file content",
//!   "create_dirs": true
//! }
//! ```

use crate::{path_safety::PathSafetyPolicy, ToolError};
use agent_core::types::{ToolCall, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;

// ── ReadFileTool ──────────────────────────────────────────────────────────

/// Reads the content of a file. Returns the file content as a string.
pub struct ReadFileTool {
    policy: Arc<PathSafetyPolicy>,
}

impl ReadFileTool {
    pub fn new(policy: Arc<PathSafetyPolicy>) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl crate::registry::Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn schema(&self) -> serde_json::Value {
        json!({
            "name": "read_file",
            "description": "Read the contents of a file at the given path.",
            "x-sensitivity": "safe",
            "parameters": {
                "type": "object",
                "required": ["path"],
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute or relative path to the file to read."
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
                tool: "read_file".into(),
                reason: "missing required 'path' argument".into(),
            })?;

        let safe_path = self.policy.validate(path_str)?;

        let content = tokio::fs::read_to_string(&safe_path)
            .await
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => ToolError::InvalidArguments {
                    tool: "read_file".into(),
                    reason: format!("file not found: {}", safe_path.display()),
                },
                _ => ToolError::Io(e),
            })?;

        let size_bytes = content.len();
        Ok(ToolResult {
            call_id: call.id,
            success: true,
            output: json!({
                "path": safe_path.to_string_lossy(),
                "content": content,
                "size_bytes": size_bytes,
            }),
        })
    }
}

// ── WriteFileTool ─────────────────────────────────────────────────────────

/// Writes content to a file. Creates or overwrites. Sensitivity: High.
pub struct WriteFileTool {
    policy: Arc<PathSafetyPolicy>,
}

impl WriteFileTool {
    pub fn new(policy: Arc<PathSafetyPolicy>) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl crate::registry::Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn schema(&self) -> serde_json::Value {
        json!({
            "name": "write_file",
            "description": "Write content to a file, creating or overwriting it.",
            "x-sensitivity": "high",
            "parameters": {
                "type": "object",
                "required": ["path", "content"],
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file to write."
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write to the file."
                    },
                    "create_dirs": {
                        "type": "boolean",
                        "description": "If true, create parent directories if they don't exist.",
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
                tool: "write_file".into(),
                reason: "missing required 'path' argument".into(),
            })?;

        let content = call
            .arguments
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments {
                tool: "write_file".into(),
                reason: "missing required 'content' argument".into(),
            })?;

        let create_dirs = call
            .arguments
            .get("create_dirs")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let safe_path = self.policy.validate(path_str)?;

        if create_dirs {
            if let Some(parent) = safe_path.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(ToolError::Io)?;
            }
        }

        let bytes = content.len();
        tokio::fs::write(&safe_path, content)
            .await
            .map_err(ToolError::Io)?;

        Ok(ToolResult {
            call_id: call.id,
            success: true,
            output: json!({
                "path": safe_path.to_string_lossy(),
                "bytes_written": bytes,
            }),
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Tool;
    use tempfile::TempDir;

    fn policy_for(dir: &TempDir) -> Arc<PathSafetyPolicy> {
        Arc::new(PathSafetyPolicy::new([dir.path()]))
    }

    #[tokio::test]
    async fn read_file_success() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "hello from test").unwrap();

        let tool = ReadFileTool::new(policy_for(&dir));
        let call = ToolCall {
            id: "c1".into(),
            name: "read_file".into(),
            arguments: json!({ "path": file_path.to_string_lossy() }),
        };
        let result = tool.execute(call).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output["content"], "hello from test");
    }

    #[tokio::test]
    async fn read_file_not_found() {
        let dir = TempDir::new().unwrap();
        let tool = ReadFileTool::new(policy_for(&dir));
        let call = ToolCall {
            id: "c1".into(),
            name: "read_file".into(),
            arguments: json!({ "path": dir.path().join("missing.txt").to_string_lossy() }),
        };
        let result = tool.execute(call).await;
        assert!(matches!(result, Err(ToolError::InvalidArguments { .. })));
    }

    #[tokio::test]
    async fn read_file_blocked_outside_root() {
        let dir = TempDir::new().unwrap();
        let tool = ReadFileTool::new(policy_for(&dir));
        let call = ToolCall {
            id: "c1".into(),
            name: "read_file".into(),
            arguments: json!({ "path": "/etc/passwd" }),
        };
        let result = tool.execute(call).await;
        assert!(matches!(result, Err(ToolError::PathTraversal(_))));
    }

    #[tokio::test]
    async fn write_file_success() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("out.txt");
        let tool = WriteFileTool::new(policy_for(&dir));
        let call = ToolCall {
            id: "c2".into(),
            name: "write_file".into(),
            arguments: json!({
                "path": file_path.to_string_lossy(),
                "content": "written content"
            }),
        };
        let result = tool.execute(call).await.unwrap();
        assert!(result.success);
        assert_eq!(std::fs::read_to_string(&file_path).unwrap(), "written content");
    }

    #[tokio::test]
    async fn write_file_blocked_outside_root() {
        let dir = TempDir::new().unwrap();
        let tool = WriteFileTool::new(policy_for(&dir));
        let call = ToolCall {
            id: "c2".into(),
            name: "write_file".into(),
            arguments: json!({
                "path": "/etc/hosts",
                "content": "evil"
            }),
        };
        let result = tool.execute(call).await;
        assert!(matches!(result, Err(ToolError::PathTraversal(_))));
    }

    #[tokio::test]
    async fn write_file_creates_dirs() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("nested/deep/file.txt");
        let tool = WriteFileTool::new(policy_for(&dir));
        let call = ToolCall {
            id: "c3".into(),
            name: "write_file".into(),
            arguments: json!({
                "path": file_path.to_string_lossy(),
                "content": "deep content",
                "create_dirs": true
            }),
        };
        let result = tool.execute(call).await.unwrap();
        assert!(result.success);
        assert!(file_path.exists());
    }
}
