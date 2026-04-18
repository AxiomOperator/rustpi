//! Text search tool using regex patterns.
//!
//! Searches files or directories for lines matching a regex pattern.
//!
//! # Input schema
//! ```json
//! {
//!   "path": "/workspace/src",
//!   "pattern": "fn main",
//!   "case_sensitive": true,
//!   "max_results": 50
//! }
//! ```

use crate::{path_safety::PathSafetyPolicy, ToolError};
use agent_core::types::{ToolCall, ToolResult};
use async_trait::async_trait;
use regex::RegexBuilder;
use serde_json::{json, Value};
use std::sync::Arc;

pub struct SearchTool {
    policy: Arc<PathSafetyPolicy>,
}

impl SearchTool {
    pub fn new(policy: Arc<PathSafetyPolicy>) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl crate::registry::Tool for SearchTool {
    fn name(&self) -> &str {
        "search"
    }

    fn schema(&self) -> serde_json::Value {
        json!({
            "name": "search",
            "description": "Search a file or directory for lines matching a regex pattern.",
            "x-sensitivity": "safe",
            "parameters": {
                "type": "object",
                "required": ["path", "pattern"],
                "properties": {
                    "path": { "type": "string", "description": "File or directory to search." },
                    "pattern": { "type": "string", "description": "Regex search pattern." },
                    "case_sensitive": { "type": "boolean", "default": true },
                    "max_results": { "type": "integer", "default": 50 }
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
                tool: "search".into(),
                reason: "missing 'path'".into(),
            })?;

        let pattern_str = call
            .arguments
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments {
                tool: "search".into(),
                reason: "missing 'pattern'".into(),
            })?;

        let case_sensitive = call
            .arguments
            .get("case_sensitive")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let max_results = call
            .arguments
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(50) as usize;

        let regex = RegexBuilder::new(pattern_str)
            .case_insensitive(!case_sensitive)
            .build()
            .map_err(|e| ToolError::InvalidArguments {
                tool: "search".into(),
                reason: format!("invalid regex pattern: {e}"),
            })?;

        let safe_path = self.policy.validate(path_str)?;

        let files = if safe_path.is_dir() {
            collect_text_files(&safe_path).await?
        } else if safe_path.is_file() {
            vec![safe_path.clone()]
        } else {
            return Err(ToolError::InvalidArguments {
                tool: "search".into(),
                reason: format!("path does not exist: {}", safe_path.display()),
            });
        };

        let mut matches: Vec<Value> = Vec::new();
        'outer: for file in &files {
            let content = match tokio::fs::read_to_string(file).await {
                Ok(c) => c,
                Err(_) => continue, // skip binary/unreadable files
            };
            for (line_num, line) in content.lines().enumerate() {
                if regex.is_match(line) {
                    matches.push(json!({
                        "file": file.to_string_lossy(),
                        "line": line_num + 1,
                        "content": line,
                    }));
                    if matches.len() >= max_results {
                        break 'outer;
                    }
                }
            }
        }

        let match_count = matches.len();
        let truncated = match_count >= max_results;
        Ok(ToolResult {
            call_id: call.id,
            success: true,
            output: json!({
                "pattern": pattern_str,
                "matches": matches,
                "match_count": match_count,
                "truncated": truncated,
            }),
        })
    }
}

/// Recursively collect readable files in a directory (depth-limited to 5).
async fn collect_text_files(
    dir: &std::path::Path,
) -> Result<Vec<std::path::PathBuf>, ToolError> {
    let mut files = Vec::new();
    collect_files_inner(dir, &mut files, 0)?;
    Ok(files)
}

fn collect_files_inner(
    dir: &std::path::Path,
    out: &mut Vec<std::path::PathBuf>,
    depth: usize,
) -> Result<(), ToolError> {
    if depth > 5 {
        return Ok(());
    }
    let entries = std::fs::read_dir(dir).map_err(ToolError::Io)?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files_inner(&path, out, depth + 1)?;
        } else if path.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Tool;
    use tempfile::TempDir;

    fn policy_for(dir: &TempDir) -> Arc<PathSafetyPolicy> {
        Arc::new(PathSafetyPolicy::new([dir.path()]))
    }

    #[tokio::test]
    async fn search_finds_match() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.txt"), "hello world\nfoo bar\n").unwrap();

        let tool = SearchTool::new(policy_for(&dir));
        let call = ToolCall {
            id: "s1".into(),
            name: "search".into(),
            arguments: json!({ "path": dir.path().to_string_lossy(), "pattern": "hello" }),
        };
        let result = tool.execute(call).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output["match_count"], 1);
    }

    #[tokio::test]
    async fn search_case_insensitive() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("b.txt"), "Hello World\n").unwrap();

        let tool = SearchTool::new(policy_for(&dir));
        let call = ToolCall {
            id: "s2".into(),
            name: "search".into(),
            arguments: json!({
                "path": dir.path().to_string_lossy(),
                "pattern": "hello",
                "case_sensitive": false
            }),
        };
        let result = tool.execute(call).await.unwrap();
        assert_eq!(result.output["match_count"], 1);
    }

    #[tokio::test]
    async fn search_invalid_regex_errors() {
        let dir = TempDir::new().unwrap();
        let tool = SearchTool::new(policy_for(&dir));
        let call = ToolCall {
            id: "s3".into(),
            name: "search".into(),
            arguments: json!({ "path": dir.path().to_string_lossy(), "pattern": "[invalid" }),
        };
        assert!(matches!(
            tool.execute(call).await,
            Err(ToolError::InvalidArguments { .. })
        ));
    }

    #[tokio::test]
    async fn search_blocked_outside_root() {
        let dir = TempDir::new().unwrap();
        let tool = SearchTool::new(policy_for(&dir));
        let call = ToolCall {
            id: "s4".into(),
            name: "search".into(),
            arguments: json!({ "path": "/etc", "pattern": "root" }),
        };
        assert!(matches!(
            tool.execute(call).await,
            Err(ToolError::PathTraversal(_))
        ));
    }

    #[tokio::test]
    async fn search_no_matches_returns_empty() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("c.txt"), "nothing here\n").unwrap();
        let tool = SearchTool::new(policy_for(&dir));
        let call = ToolCall {
            id: "s5".into(),
            name: "search".into(),
            arguments: json!({ "path": dir.path().to_string_lossy(), "pattern": "xyz123" }),
        };
        let result = tool.execute(call).await.unwrap();
        assert_eq!(result.output["match_count"], 0);
    }
}
