//! Normalize `AgentEvent`s from the event bus into `RpcEvent`s for external consumption.

use agent_core::types::AgentEvent;
use serde_json::json;

use crate::protocol::{EventCategory, RpcEvent};

/// Convert an `AgentEvent` from the bus into a normalized `RpcEvent`.
pub fn normalize_event(event: &AgentEvent, seq: u64) -> RpcEvent {
    let (timestamp, category, session_id, run_id, event_type, payload) = match event {
        // --- Session lifecycle ---
        AgentEvent::SessionCreated { session_id, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Session,
            Some(session_id.to_string()),
            None,
            "session_created",
            json!({}),
        ),
        AgentEvent::SessionResumed { session_id, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Session,
            Some(session_id.to_string()),
            None,
            "session_resumed",
            json!({}),
        ),
        AgentEvent::SessionEnded { session_id, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Session,
            Some(session_id.to_string()),
            None,
            "session_ended",
            json!({}),
        ),

        // --- Run lifecycle ---
        AgentEvent::RunCreated { run_id, session_id, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Run,
            Some(session_id.to_string()),
            Some(run_id.to_string()),
            "run_created",
            json!({}),
        ),
        AgentEvent::RunQueued { run_id, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Run,
            None,
            Some(run_id.to_string()),
            "run_queued",
            json!({}),
        ),
        AgentEvent::RunStarted { run_id, session_id, provider, model, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Run,
            Some(session_id.to_string()),
            Some(run_id.to_string()),
            "run_started",
            json!({ "provider": provider.to_string(), "model": model.to_string() }),
        ),
        AgentEvent::RunCompleted { run_id, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Run,
            None,
            Some(run_id.to_string()),
            "run_completed",
            json!({}),
        ),
        AgentEvent::RunCancelled { run_id, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Run,
            None,
            Some(run_id.to_string()),
            "run_cancelled",
            json!({}),
        ),
        AgentEvent::RunFailed { run_id, reason, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Run,
            None,
            Some(run_id.to_string()),
            "run_failed",
            json!({ "reason": reason }),
        ),
        AgentEvent::InterruptRequested { run_id, reason, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Run,
            None,
            Some(run_id.to_string()),
            "interrupt_requested",
            json!({ "reason": reason }),
        ),
        AgentEvent::CancellationRequested { run_id, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Run,
            None,
            Some(run_id.to_string()),
            "cancellation_requested",
            json!({}),
        ),

        // --- Prompt ---
        AgentEvent::PromptAssembled { run_id, section_count, estimated_tokens, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Run,
            None,
            Some(run_id.to_string()),
            "prompt_assembled",
            json!({ "section_count": section_count, "estimated_tokens": estimated_tokens }),
        ),

        // --- Model output ---
        AgentEvent::TokenChunk { run_id, delta, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Run,
            None,
            Some(run_id.to_string()),
            "token_chunk",
            json!({ "delta": delta }),
        ),
        AgentEvent::ToolCallRequested { run_id, call, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Run,
            None,
            Some(run_id.to_string()),
            "tool_call_requested",
            json!({ "call_id": call.id, "tool_name": call.name }),
        ),
        AgentEvent::ToolResultSubmitted { run_id, result, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Run,
            None,
            Some(run_id.to_string()),
            "tool_result_submitted",
            json!({ "call_id": result.call_id, "success": result.success }),
        ),

        // --- Tool execution (fine-grained) ---
        AgentEvent::ToolExecutionStarted { run_id, call_id, tool_name, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Tool,
            None,
            Some(run_id.to_string()),
            "tool_execution_started",
            json!({ "call_id": call_id, "tool_name": tool_name }),
        ),
        AgentEvent::ToolStdout { run_id, call_id, line, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Tool,
            None,
            Some(run_id.to_string()),
            "tool_stdout",
            json!({ "call_id": call_id, "line": line }),
        ),
        AgentEvent::ToolStderr { run_id, call_id, line, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Tool,
            None,
            Some(run_id.to_string()),
            "tool_stderr",
            json!({ "call_id": call_id, "line": line }),
        ),
        AgentEvent::ToolExecutionCompleted { run_id, call_id, exit_code, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Tool,
            None,
            Some(run_id.to_string()),
            "tool_execution_completed",
            json!({ "call_id": call_id, "exit_code": exit_code }),
        ),
        AgentEvent::ToolExecutionFailed { run_id, call_id, reason, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Tool,
            None,
            Some(run_id.to_string()),
            "tool_execution_failed",
            json!({ "call_id": call_id, "reason": reason }),
        ),
        AgentEvent::ToolExecutionCancelled { run_id, call_id, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Tool,
            None,
            Some(run_id.to_string()),
            "tool_execution_cancelled",
            json!({ "call_id": call_id }),
        ),

        // --- Tool lifecycle ---
        AgentEvent::ToolStarted { run_id, call_id, tool_name, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Tool,
            None,
            Some(run_id.to_string()),
            "tool_started",
            json!({ "call_id": call_id, "tool_name": tool_name }),
        ),
        AgentEvent::ToolCompleted { run_id, call_id, tool_name, exit_code, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Tool,
            None,
            Some(run_id.to_string()),
            "tool_completed",
            json!({ "call_id": call_id, "tool_name": tool_name, "exit_code": exit_code }),
        ),
        AgentEvent::ToolCancelled { run_id, call_id, tool_name, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Tool,
            None,
            Some(run_id.to_string()),
            "tool_cancelled",
            json!({ "call_id": call_id, "tool_name": tool_name }),
        ),
        AgentEvent::ToolFailed { run_id, call_id, tool_name, reason, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Tool,
            None,
            Some(run_id.to_string()),
            "tool_failed",
            json!({ "call_id": call_id, "tool_name": tool_name, "reason": reason }),
        ),

        // --- Auth ---
        AgentEvent::AuthStateChanged { provider, state, timestamp } => {
            let state_str = match state {
                agent_core::types::AuthState::Authenticated { .. } => "authenticated",
                agent_core::types::AuthState::Unauthenticated => "unauthenticated",
                agent_core::types::AuthState::Pending { .. } => "pending",
                agent_core::types::AuthState::Expired { .. } => "expired",
                agent_core::types::AuthState::Failed { .. } => "failed",
            };
            (
                timestamp.to_rfc3339(),
                EventCategory::Auth,
                None,
                None,
                "auth_state_changed",
                json!({ "provider": provider.to_string(), "state": state_str }),
            )
        }
        AgentEvent::AuthLoginStarted { provider, timestamp, .. } => (
            timestamp.to_rfc3339(),
            EventCategory::Auth,
            None,
            None,
            "auth_login_started",
            json!({ "provider": provider.to_string() }),
        ),
        AgentEvent::AuthLoginCompleted { provider, timestamp, .. } => (
            timestamp.to_rfc3339(),
            EventCategory::Auth,
            None,
            None,
            "auth_login_completed",
            json!({ "provider": provider.to_string() }),
        ),
        AgentEvent::AuthLoginFailed { provider, reason, timestamp, .. } => (
            timestamp.to_rfc3339(),
            EventCategory::Auth,
            None,
            None,
            "auth_login_failed",
            json!({ "provider": provider.to_string(), "reason": reason }),
        ),
        AgentEvent::DeviceFlowInitiated {
            provider,
            verification_uri,
            user_code,
            expires_in_secs,
            timestamp,
        } => (
            timestamp.to_rfc3339(),
            EventCategory::Auth,
            None,
            None,
            "device_flow_initiated",
            json!({
                "provider": provider.to_string(),
                "verification_uri": verification_uri,
                "user_code": user_code,
                "expires_in_secs": expires_in_secs,
            }),
        ),
        AgentEvent::DeviceCodeIssued { provider, interval_secs, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Auth,
            None,
            None,
            "device_code_issued",
            json!({ "provider": provider.to_string(), "interval_secs": interval_secs }),
        ),
        AgentEvent::TokenStored { provider, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Auth,
            None,
            None,
            "token_stored",
            json!({ "provider": provider.to_string() }),
        ),
        AgentEvent::TokenRefreshed { provider, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Auth,
            None,
            None,
            "token_refreshed",
            json!({ "provider": provider.to_string() }),
        ),
        AgentEvent::TokenRefreshFailed { provider, reason, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Auth,
            None,
            None,
            "token_refresh_failed",
            json!({ "provider": provider.to_string(), "reason": reason }),
        ),
        AgentEvent::AuthStateLoaded { provider, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Auth,
            None,
            None,
            "auth_state_loaded",
            json!({ "provider": provider.to_string() }),
        ),
        AgentEvent::AuthStateCleared { provider, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Auth,
            None,
            None,
            "auth_state_cleared",
            json!({ "provider": provider.to_string() }),
        ),

        // --- Context ---
        AgentEvent::ContextBuilt { run_id, token_count, file_count, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Run,
            None,
            Some(run_id.to_string()),
            "context_built",
            json!({ "token_count": token_count, "file_count": file_count }),
        ),
        AgentEvent::ContextCompacted { run_id, tokens_before, tokens_after, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Run,
            None,
            Some(run_id.to_string()),
            "context_compacted",
            json!({ "tokens_before": tokens_before, "tokens_after": tokens_after }),
        ),

        // --- Security audit ---
        AgentEvent::ApprovalDenied { run_id, tool_name, sensitivity, reason, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Tool,
            None,
            Some(run_id.to_string()),
            "approval_denied",
            json!({ "tool_name": tool_name, "sensitivity": sensitivity, "reason": reason }),
        ),
        AgentEvent::ApprovalGranted { run_id, tool_name, sensitivity, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Tool,
            None,
            Some(run_id.to_string()),
            "approval_granted",
            json!({ "tool_name": tool_name, "sensitivity": sensitivity }),
        ),
        AgentEvent::CommandDenied { run_id, command_preview, reason, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Tool,
            None,
            Some(run_id.to_string()),
            "command_denied",
            json!({ "command_preview": command_preview, "reason": reason }),
        ),
        AgentEvent::PathDenied { run_id, path, reason, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Tool,
            None,
            Some(run_id.to_string()),
            "path_denied",
            json!({ "path": path, "reason": reason }),
        ),
        AgentEvent::OverwriteBlocked { run_id, path, reason, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::Tool,
            None,
            Some(run_id.to_string()),
            "overwrite_blocked",
            json!({ "path": path, "reason": reason }),
        ),
        AgentEvent::PolicyDenied { domain, subject, rule, reason, timestamp } => (
            timestamp.to_rfc3339(),
            EventCategory::System,
            None,
            None,
            "policy_denied",
            json!({ "domain": domain, "subject": subject, "rule": rule, "reason": reason }),
        ),
    };

    RpcEvent {
        seq,
        timestamp,
        category,
        session_id,
        run_id,
        event_type: event_type.to_string(),
        payload,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::types::{ModelId, ProviderId, RunId, SessionId};
    use chrono::Utc;

    #[test]
    fn token_chunk_normalized() {
        let run_id = RunId::new();
        let event = AgentEvent::TokenChunk {
            run_id: run_id.clone(),
            delta: "hello world".into(),
            timestamp: Utc::now(),
        };
        let rpc = normalize_event(&event, 7);
        assert_eq!(rpc.seq, 7);
        assert_eq!(rpc.event_type, "token_chunk");
        assert!(matches!(rpc.category, EventCategory::Run));
        assert_eq!(rpc.run_id, Some(run_id.to_string()));
        assert_eq!(rpc.payload["delta"], "hello world");
    }

    #[test]
    fn run_started_normalized() {
        let run_id = RunId::new();
        let session_id = SessionId::new();
        let event = AgentEvent::RunStarted {
            run_id: run_id.clone(),
            session_id: session_id.clone(),
            provider: ProviderId::new("openai"),
            model: ModelId::new("gpt-4o"),
            timestamp: Utc::now(),
        };
        let rpc = normalize_event(&event, 1);
        assert_eq!(rpc.event_type, "run_started");
        assert!(matches!(rpc.category, EventCategory::Run));
        assert_eq!(rpc.session_id, Some(session_id.to_string()));
        assert_eq!(rpc.run_id, Some(run_id.to_string()));
        assert_eq!(rpc.payload["provider"], "openai");
        assert_eq!(rpc.payload["model"], "gpt-4o");
    }

    #[test]
    fn session_id_extracted_correctly() {
        let session_id = SessionId::new();
        let event = AgentEvent::SessionCreated {
            session_id: session_id.clone(),
            timestamp: Utc::now(),
        };
        let rpc = normalize_event(&event, 0);
        assert_eq!(rpc.session_id, Some(session_id.to_string()));
        assert!(rpc.run_id.is_none());
        assert!(matches!(rpc.category, EventCategory::Session));
    }
}
