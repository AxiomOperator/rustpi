//! Tool-call placeholder orchestration.
//!
//! This module provides the in-runtime representation of pending tool calls
//! and the orchestration hooks that integrate with the run loop.
//!
//! # Phase 1 scope
//! - Track pending tool calls per run
//! - Emit [`AgentEvent::ToolCallRequested`] when the model requests a tool
//! - Accept tool results and emit [`AgentEvent::ToolResultSubmitted`]
//! - Expose cancellation of pending calls
//!
//! Full subprocess execution is implemented in the `tool-runtime` crate (Phase 5).
//! This module handles the run-loop side of the handshake only.

use crate::{
    error::AgentError,
    types::{AgentEvent, RunId, ToolCall, ToolResult},
};
use chrono::Utc;
use std::collections::HashMap;

/// Lifecycle state of a pending tool call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingCallStatus {
    /// Requested by the model; awaiting execution by `tool-runtime`.
    Requested,
    /// Execution is in progress.
    Executing,
    /// Execution completed; result has been submitted to the model.
    Completed,
    /// Execution was cancelled.
    Cancelled,
}

/// A tool call that has been requested by the model and is tracked by the runtime.
#[derive(Debug, Clone)]
pub struct PendingToolCall {
    pub call: ToolCall,
    pub run_id: RunId,
    pub status: PendingCallStatus,
}

/// Manages in-flight tool calls for a single run.
///
/// One [`ToolOrchestrator`] is created per run and lives until the run
/// is complete or cancelled.
#[derive(Debug, Default)]
pub struct ToolOrchestrator {
    pending: HashMap<String, PendingToolCall>,
}

impl ToolOrchestrator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a tool call requested by the model.
    ///
    /// Emits [`AgentEvent::ToolCallRequested`] for the event bus.
    /// Returns an error if a call with the same ID is already registered.
    pub fn request_call(
        &mut self,
        run_id: RunId,
        call: ToolCall,
    ) -> Result<AgentEvent, AgentError> {
        if self.pending.contains_key(&call.id) {
            return Err(AgentError::PromptAssembly(format!(
                "tool call id '{}' already registered",
                call.id
            )));
        }
        let event = AgentEvent::ToolCallRequested {
            run_id: run_id.clone(),
            call: call.clone(),
            timestamp: Utc::now(),
        };
        self.pending.insert(
            call.id.clone(),
            PendingToolCall {
                call,
                run_id,
                status: PendingCallStatus::Requested,
            },
        );
        Ok(event)
    }

    /// Mark a call as executing (transitioned to `tool-runtime`).
    pub fn mark_executing(&mut self, call_id: &str) -> Result<(), AgentError> {
        let entry = self
            .pending
            .get_mut(call_id)
            .ok_or_else(|| AgentError::PromptAssembly(format!("call '{}' not found", call_id)))?;
        entry.status = PendingCallStatus::Executing;
        Ok(())
    }

    /// Submit the result of a completed tool call.
    ///
    /// Emits [`AgentEvent::ToolResultSubmitted`] and removes the call from pending.
    pub fn submit_result(
        &mut self,
        run_id: RunId,
        result: ToolResult,
    ) -> Result<AgentEvent, AgentError> {
        let entry = self.pending.get_mut(&result.call_id).ok_or_else(|| {
            AgentError::PromptAssembly(format!("call '{}' not found", result.call_id))
        })?;
        entry.status = PendingCallStatus::Completed;
        let event = AgentEvent::ToolResultSubmitted {
            run_id,
            result,
            timestamp: Utc::now(),
        };
        Ok(event)
    }

    /// Cancel all pending tool calls for this run.
    ///
    /// Returns the IDs of calls that were cancelled.
    pub fn cancel_all(&mut self) -> Vec<String> {
        let mut cancelled = Vec::new();
        for (id, call) in self.pending.iter_mut() {
            if call.status == PendingCallStatus::Requested
                || call.status == PendingCallStatus::Executing
            {
                call.status = PendingCallStatus::Cancelled;
                cancelled.push(id.clone());
            }
        }
        cancelled
    }

    /// Number of tool calls with `Requested` or `Executing` status.
    pub fn active_count(&self) -> usize {
        self.pending
            .values()
            .filter(|c| {
                c.status == PendingCallStatus::Requested || c.status == PendingCallStatus::Executing
            })
            .count()
    }

    /// Returns `true` if there are no pending tool calls awaiting execution.
    pub fn is_idle(&self) -> bool {
        self.active_count() == 0
    }

    /// Look up a specific pending call by ID.
    pub fn get(&self, call_id: &str) -> Option<&PendingToolCall> {
        self.pending.get(call_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_call(id: &str, name: &str) -> ToolCall {
        ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            arguments: json!({"path": "src/main.rs"}),
        }
    }

    #[test]
    fn request_call_emits_event() {
        let mut orch = ToolOrchestrator::new();
        let run_id = RunId::new();
        let call = make_call("call_1", "read_file");
        let event = orch.request_call(run_id.clone(), call).unwrap();
        assert!(matches!(event, AgentEvent::ToolCallRequested { .. }));
        assert_eq!(orch.active_count(), 1);
    }

    #[test]
    fn duplicate_call_id_errors() {
        let mut orch = ToolOrchestrator::new();
        let run_id = RunId::new();
        let call = make_call("call_1", "read_file");
        orch.request_call(run_id.clone(), call.clone()).unwrap();
        assert!(orch.request_call(run_id, call).is_err());
    }

    #[test]
    fn submit_result_emits_event() {
        let mut orch = ToolOrchestrator::new();
        let run_id = RunId::new();
        orch.request_call(run_id.clone(), make_call("call_1", "read_file"))
            .unwrap();
        let result = ToolResult {
            call_id: "call_1".to_string(),
            success: true,
            output: json!("file contents here"),
        };
        let event = orch.submit_result(run_id, result).unwrap();
        assert!(matches!(event, AgentEvent::ToolResultSubmitted { .. }));
        // Call is completed; no longer active.
        assert_eq!(orch.active_count(), 0);
    }

    #[test]
    fn submit_result_unknown_call_errors() {
        let mut orch = ToolOrchestrator::new();
        let result = ToolResult {
            call_id: "nonexistent".to_string(),
            success: false,
            output: json!(null),
        };
        assert!(orch.submit_result(RunId::new(), result).is_err());
    }

    #[test]
    fn cancel_all_pending() {
        let mut orch = ToolOrchestrator::new();
        let run_id = RunId::new();
        orch.request_call(run_id.clone(), make_call("call_1", "read_file"))
            .unwrap();
        orch.request_call(run_id.clone(), make_call("call_2", "write_file"))
            .unwrap();
        assert_eq!(orch.active_count(), 2);
        let cancelled = orch.cancel_all();
        assert_eq!(cancelled.len(), 2);
        assert_eq!(orch.active_count(), 0);
    }

    #[test]
    fn cancel_all_skips_completed() {
        let mut orch = ToolOrchestrator::new();
        let run_id = RunId::new();
        orch.request_call(run_id.clone(), make_call("call_1", "read_file"))
            .unwrap();
        // Complete call_1.
        let result = ToolResult {
            call_id: "call_1".to_string(),
            success: true,
            output: json!("ok"),
        };
        orch.submit_result(run_id.clone(), result).unwrap();
        // Add another pending.
        orch.request_call(run_id.clone(), make_call("call_2", "search"))
            .unwrap();
        let cancelled = orch.cancel_all();
        assert_eq!(cancelled.len(), 1);
        assert_eq!(cancelled[0], "call_2");
    }

    #[test]
    fn mark_executing_transitions_status() {
        let mut orch = ToolOrchestrator::new();
        let run_id = RunId::new();
        orch.request_call(run_id, make_call("call_1", "shell")).unwrap();
        orch.mark_executing("call_1").unwrap();
        assert_eq!(
            orch.get("call_1").unwrap().status,
            PendingCallStatus::Executing
        );
    }

    #[test]
    fn is_idle_with_no_pending() {
        let orch = ToolOrchestrator::new();
        assert!(orch.is_idle());
    }
}
