//! Unified tool runner: dispatches all tool calls through policy, approval, timeout,
//! cancellation, and event emission.

use crate::{
    approval::{ApprovalContext, ApprovalDecision, ApprovalHook, AutoApprove},
    registry::ToolRegistry,
    schema::{ToolConfig, ToolSensitivity},
    ToolError,
};
use agent_core::types::{AgentEvent, RunId, ToolCall, ToolResult};
use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

/// Default tool execution timeout.
pub const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// The central tool runner. All tool executions go through this.
pub struct ToolRunner {
    registry: Arc<ToolRegistry>,
    default_timeout: Duration,
    approval: Arc<dyn ApprovalHook>,
    /// Optional broadcast sender for tool lifecycle events.
    event_tx: Option<broadcast::Sender<AgentEvent>>,
}

impl ToolRunner {
    pub fn new(registry: Arc<ToolRegistry>, default_timeout: Duration) -> Self {
        Self {
            registry,
            default_timeout,
            approval: Arc::new(AutoApprove),
            event_tx: None,
        }
    }

    /// Set a custom approval hook.
    pub fn with_approval(mut self, hook: Arc<dyn ApprovalHook>) -> Self {
        self.approval = hook;
        self
    }

    /// Connect an event broadcast channel for lifecycle events.
    pub fn with_event_tx(mut self, tx: broadcast::Sender<AgentEvent>) -> Self {
        self.event_tx = Some(tx);
        self
    }

    fn emit(&self, event: AgentEvent) {
        if let Some(tx) = &self.event_tx {
            let _ = tx.send(event); // ignore if no subscribers
        }
    }

    /// Execute a tool call with the provided config.
    pub async fn execute(
        &self,
        call: ToolCall,
        config: ToolConfig,
    ) -> Result<ToolResult, ToolError> {
        let tool = self
            .registry
            .get(&call.name)
            .ok_or_else(|| ToolError::NotFound(call.name.clone()))?;

        let schema = tool.schema();
        // Determine sensitivity from schema metadata if present.
        // Tools can advertise sensitivity via an "x-sensitivity" field in their schema.
        let sensitivity = schema
            .get("x-sensitivity")
            .and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_str::<ToolSensitivity>(&format!("\"{}\"", s)).ok())
            .unwrap_or(ToolSensitivity::Safe);

        let needs_approval =
            config.require_approval || sensitivity >= ToolSensitivity::High;

        if needs_approval {
            let ctx = ApprovalContext {
                tool_name: call.name.clone(),
                sensitivity: sensitivity.clone(),
                args: call.arguments.clone(),
            };
            match self.approval.check(&ctx).await {
                ApprovalDecision::Approved => {}
                ApprovalDecision::Denied(reason) | ApprovalDecision::Pending(reason) => {
                    let run_id = config.run_id.clone().unwrap_or_else(RunId::new);
                    self.emit(AgentEvent::ToolFailed {
                        run_id,
                        call_id: call.id.clone(),
                        tool_name: call.name.clone(),
                        reason: format!("approval denied: {}", reason),
                        timestamp: Utc::now(),
                    });
                    return Err(ToolError::PolicyDenied(reason));
                }
            }
        }

        let run_id = config.run_id.clone().unwrap_or_else(RunId::new);

        self.emit(AgentEvent::ToolStarted {
            run_id: run_id.clone(),
            call_id: call.id.clone(),
            tool_name: call.name.clone(),
            timestamp: Utc::now(),
        });

        let timeout = config.timeout.unwrap_or(self.default_timeout);
        let cancel = config.cancel.clone();

        let result = if let Some(cancel_token) = cancel {
            tokio::select! {
                r = tokio::time::timeout(timeout, tool.execute(call.clone())) => {
                    r.map_err(|_| ToolError::Timeout(timeout.as_secs()))?
                }
                _ = cancel_token.cancelled() => {
                    self.emit(AgentEvent::ToolCancelled {
                        run_id: run_id.clone(),
                        call_id: call.id.clone(),
                        tool_name: call.name.clone(),
                        timestamp: Utc::now(),
                    });
                    return Err(ToolError::Cancelled);
                }
            }
        } else {
            tokio::time::timeout(timeout, tool.execute(call.clone()))
                .await
                .map_err(|_| ToolError::Timeout(timeout.as_secs()))?
        };

        match result {
            Ok(tool_result) => {
                let exit_code = tool_result
                    .output
                    .get("exit_code")
                    .and_then(|v| v.as_i64())
                    .map(|c| c as i32);

                self.emit(AgentEvent::ToolCompleted {
                    run_id,
                    call_id: tool_result.call_id.clone(),
                    tool_name: call.name.clone(),
                    exit_code,
                    timestamp: Utc::now(),
                });
                Ok(tool_result)
            }
            Err(e) => {
                self.emit(AgentEvent::ToolFailed {
                    run_id,
                    call_id: call.id.clone(),
                    tool_name: call.name.clone(),
                    reason: e.to_string(),
                    timestamp: Utc::now(),
                });
                Err(e)
            }
        }
    }

    /// Execute with only a RunId (no cancellation token, default timeout).
    pub async fn execute_simple(
        &self,
        call: ToolCall,
        run_id: RunId,
    ) -> Result<ToolResult, ToolError> {
        self.execute(
            call,
            ToolConfig {
                run_id: Some(run_id),
                ..Default::default()
            },
        )
        .await
    }
}
