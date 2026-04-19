//! Event record envelope and audit record types.

use crate::AgentEvent;
use serde::{Deserialize, Serialize};

/// A stable envelope wrapping an AgentEvent for storage.
/// The envelope adds storage metadata without modifying the event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    /// Monotonically increasing sequence number (per log/store).
    pub seq: u64,
    /// Wall-clock time the record was appended (may differ from event timestamp).
    pub appended_at: chrono::DateTime<chrono::Utc>,
    /// The wrapped event.
    pub event: AgentEvent,
}

/// An audit record for significant security/compliance events.
/// Audit records are a subset of EventRecords marked for audit retention.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRecord {
    pub seq: u64,
    pub appended_at: chrono::DateTime<chrono::Utc>,
    pub event: AgentEvent,
    /// Additional audit context.
    pub audit_kind: AuditKind,
    /// Who/what triggered the event.
    pub actor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditKind {
    SessionLifecycle,
    RunLifecycle,
    ToolExecution,
    FileMutation,
    AuthEvent,
    PolicyDecision,
}

impl EventRecord {
    pub fn new(seq: u64, event: AgentEvent) -> Self {
        Self {
            seq,
            appended_at: chrono::Utc::now(),
            event,
        }
    }

    /// Returns true for events that should generate an audit record.
    pub fn is_audit_relevant(&self) -> bool {
        matches!(
            self.event,
            AgentEvent::SessionCreated { .. }
                | AgentEvent::SessionEnded { .. }
                | AgentEvent::RunStarted { .. }
                | AgentEvent::RunCompleted { .. }
                | AgentEvent::RunCancelled { .. }
                | AgentEvent::RunFailed { .. }
                | AgentEvent::ToolCallRequested { .. }
                | AgentEvent::ToolResultSubmitted { .. }
                | AgentEvent::CancellationRequested { .. }
                | AgentEvent::ApprovalDenied { .. }
                | AgentEvent::ApprovalGranted { .. }
                | AgentEvent::CommandDenied { .. }
                | AgentEvent::PathDenied { .. }
                | AgentEvent::OverwriteBlocked { .. }
                | AgentEvent::PolicyDenied { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::types::{ModelId, ProviderId, RunId, SessionId};
    use chrono::Utc;

    #[test]
    fn event_record_roundtrip() {
        let session_id = SessionId::new();
        let record = EventRecord::new(
            0,
            AgentEvent::SessionCreated {
                session_id: session_id.clone(),
                timestamp: Utc::now(),
            },
        );
        let json = serde_json::to_string(&record).unwrap();
        let decoded: EventRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.seq, 0);
        match decoded.event {
            AgentEvent::SessionCreated { session_id: sid, .. } => {
                assert_eq!(sid, session_id);
            }
            _ => panic!("wrong event type"),
        }
    }

    #[test]
    fn is_audit_relevant_true_for_session_created() {
        let r = EventRecord::new(
            0,
            AgentEvent::SessionCreated {
                session_id: SessionId::new(),
                timestamp: Utc::now(),
            },
        );
        assert!(r.is_audit_relevant());
    }

    #[test]
    fn is_audit_relevant_true_for_run_started() {
        let r = EventRecord::new(
            1,
            AgentEvent::RunStarted {
                run_id: RunId::new(),
                session_id: SessionId::new(),
                provider: ProviderId::new("openai"),
                model: ModelId::new("gpt-4o"),
                timestamp: Utc::now(),
            },
        );
        assert!(r.is_audit_relevant());
    }

    #[test]
    fn is_audit_relevant_false_for_prompt_assembled() {
        let r = EventRecord::new(
            2,
            AgentEvent::PromptAssembled {
                run_id: RunId::new(),
                section_count: 3,
                estimated_tokens: 512,
                timestamp: Utc::now(),
            },
        );
        assert!(!r.is_audit_relevant());
    }

    #[test]
    fn is_audit_relevant_false_for_run_queued() {
        let r = EventRecord::new(
            3,
            AgentEvent::RunQueued {
                run_id: RunId::new(),
                timestamp: Utc::now(),
            },
        );
        assert!(!r.is_audit_relevant());
    }

    #[test]
    fn audit_kind_serializes_snake_case() {
        let json = serde_json::to_string(&AuditKind::SessionLifecycle).unwrap();
        assert_eq!(json, r#""session_lifecycle""#);
    }
}
