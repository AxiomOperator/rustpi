//! Shell subprocess tool.
//!
//! Executes a command in a subprocess. This tool has `Critical` sensitivity
//! and always requires approval unless the runtime's approval hook approves it.
//!
//! # Input schema
//! ```json
//! {
//!   "command": "echo hello",   // shell command string
//!   "timeout_secs": 30         // optional per-call timeout override
//! }
//! ```
//!
//! # Output
//! JSON object with: `exit_code`, `stdout`, `stderr`, `success`, `reason`

use crate::{
    subprocess::{run_subprocess, SubprocessConfig, TerminationReason},
    ToolError,
};
use agent_core::types::{AgentEvent, ToolCall, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

/// Default timeout for shell commands.
const DEFAULT_SHELL_TIMEOUT_SECS: u64 = 30;

/// Tool that executes shell commands via a subprocess.
pub struct ShellTool {
    /// Optional event sender for streaming stdout/stderr.
    event_tx: Option<broadcast::Sender<AgentEvent>>,
    /// Optional cancellation token for default cancellation support (reserved for future use).
    _default_cancel: Option<CancellationToken>,
}

impl ShellTool {
    pub fn new() -> Self {
        Self { event_tx: None, _default_cancel: None }
    }

    pub fn with_event_tx(mut self, tx: broadcast::Sender<AgentEvent>) -> Self {
        self.event_tx = Some(tx);
        self
    }
}

impl Default for ShellTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl crate::registry::Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }

    fn schema(&self) -> serde_json::Value {
        json!({
            "name": "shell",
            "description": "Execute a shell command in a subprocess. Returns stdout, stderr, and exit code.",
            "x-sensitivity": "critical",
            "parameters": {
                "type": "object",
                "required": ["command"],
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command to execute."
                    },
                    "timeout_secs": {
                        "type": "integer",
                        "description": "Optional per-call timeout override in seconds."
                    }
                }
            }
        })
    }

    async fn execute(&self, call: ToolCall) -> Result<ToolResult, ToolError> {
        let command = call.arguments.get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments {
                tool: "shell".into(),
                reason: "missing required 'command' argument".into(),
            })?
            .to_string();

        let timeout_secs = call.arguments.get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_SHELL_TIMEOUT_SECS);

        let result = run_subprocess(SubprocessConfig {
            program: "sh".into(),
            args: vec!["-c".into(), command],
            working_dir: None,
            env: vec![],
            timeout: Duration::from_secs(timeout_secs),
            cancel: None,
            event_tx: self.event_tx.clone(),
            run_id: None,
            call_id: Some(call.id.clone()),
            redactor: None,
        })
        .await
        .map_err(ToolError::Io)?;

        let success = result.success();
        let reason_str = match result.reason {
            TerminationReason::Exited => "exited",
            TerminationReason::TimedOut => "timed_out",
            TerminationReason::Cancelled => "cancelled",
        };

        if result.reason == TerminationReason::TimedOut {
            return Err(ToolError::Timeout(timeout_secs));
        }
        if result.reason == TerminationReason::Cancelled {
            return Err(ToolError::Cancelled);
        }

        Ok(ToolResult {
            call_id: call.id,
            success,
            output: json!({
                "exit_code": result.exit_code,
                "stdout": result.stdout,
                "stderr": result.stderr,
                "success": success,
                "reason": reason_str,
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Tool;

    #[tokio::test]
    async fn shell_tool_runs_echo() {
        let tool = ShellTool::new();
        let call = ToolCall {
            id: "c1".into(),
            name: "shell".into(),
            arguments: json!({"command": "echo hello"}),
        };
        let result = tool.execute(call).await.unwrap();
        assert!(result.success);
        assert!(result.output["stdout"].as_str().unwrap().contains("hello"));
    }

    #[tokio::test]
    async fn shell_tool_captures_nonzero_exit() {
        let tool = ShellTool::new();
        let call = ToolCall {
            id: "c2".into(),
            name: "shell".into(),
            arguments: json!({"command": "exit 1"}),
        };
        let result = tool.execute(call).await.unwrap();
        assert!(!result.success);
        assert_eq!(result.output["exit_code"], 1);
    }

    #[tokio::test]
    async fn shell_tool_timeout() {
        let tool = ShellTool::new();
        let call = ToolCall {
            id: "c3".into(),
            name: "shell".into(),
            arguments: json!({"command": "sleep 10", "timeout_secs": 0}),
        };
        let result = tool.execute(call).await;
        assert!(matches!(result, Err(ToolError::Timeout(_))));
    }

    #[test]
    fn shell_tool_name() {
        assert_eq!(ShellTool::new().name(), "shell");
    }

    #[test]
    fn schema_has_sensitivity() {
        let tool = ShellTool::new();
        let schema = tool.schema();
        assert_eq!(schema["x-sensitivity"], "critical");
    }
}
