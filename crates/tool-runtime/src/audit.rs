//! Security audit event sink.
//!
//! Tools and policy engines can emit security audit events through this sink,
//! which routes them to the event bus and/or a structured audit log.

use agent_core::types::{AgentEvent, RunId};
use chrono::Utc;
use tokio::sync::broadcast;

/// Sink that emits security audit events onto the event bus.
pub struct AuditSink {
    tx: Option<broadcast::Sender<AgentEvent>>,
}

impl AuditSink {
    pub fn new(tx: broadcast::Sender<AgentEvent>) -> Self {
        Self { tx: Some(tx) }
    }

    /// No-op sink that discards all events. Used as a safe default.
    pub fn noop() -> Self {
        Self { tx: None }
    }

    fn emit(&self, event: AgentEvent) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(event);
        }
    }

    pub fn command_denied(&self, run_id: RunId, command_preview: &str, reason: &str) {
        self.emit(AgentEvent::CommandDenied {
            run_id,
            command_preview: command_preview.chars().take(100).collect(),
            reason: reason.to_owned(),
            timestamp: Utc::now(),
        });
    }

    pub fn path_denied(&self, run_id: RunId, path: &str, reason: &str) {
        self.emit(AgentEvent::PathDenied {
            run_id,
            path: path.to_owned(),
            reason: reason.to_owned(),
            timestamp: Utc::now(),
        });
    }

    pub fn overwrite_blocked(&self, run_id: RunId, path: &str, reason: &str) {
        self.emit(AgentEvent::OverwriteBlocked {
            run_id,
            path: path.to_owned(),
            reason: reason.to_owned(),
            timestamp: Utc::now(),
        });
    }

    pub fn approval_denied(&self, run_id: RunId, tool_name: &str, sensitivity: &str, reason: &str) {
        self.emit(AgentEvent::ApprovalDenied {
            run_id,
            tool_name: tool_name.to_owned(),
            sensitivity: sensitivity.to_owned(),
            reason: reason.to_owned(),
            timestamp: Utc::now(),
        });
    }

    pub fn approval_granted(&self, run_id: RunId, tool_name: &str, sensitivity: &str) {
        self.emit(AgentEvent::ApprovalGranted {
            run_id,
            tool_name: tool_name.to_owned(),
            sensitivity: sensitivity.to_owned(),
            timestamp: Utc::now(),
        });
    }

    pub fn policy_denied(&self, domain: &str, subject: &str, rule: Option<&str>, reason: &str) {
        self.emit(AgentEvent::PolicyDenied {
            domain: domain.to_owned(),
            subject: subject.to_owned(),
            rule: rule.map(str::to_owned),
            reason: reason.to_owned(),
            timestamp: Utc::now(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::types::AgentEvent;

    #[test]
    fn noop_sink_does_not_panic() {
        let sink = AuditSink::noop();
        let run_id = RunId::new();
        sink.command_denied(run_id.clone(), "rm -rf /", "blocked");
        sink.path_denied(run_id.clone(), "/etc/passwd", "traversal");
        sink.overwrite_blocked(run_id.clone(), "/etc/hosts", "protected");
        sink.approval_denied(run_id.clone(), "shell", "critical", "requires approval");
        sink.approval_granted(run_id.clone(), "read_file", "safe");
        sink.policy_denied("tool", "shell", Some("no-shell"), "disallowed");
    }

    #[tokio::test]
    async fn command_denied_emits_event() {
        let (tx, mut rx) = broadcast::channel(16);
        let sink = AuditSink::new(tx);
        let run_id = RunId::new();
        sink.command_denied(run_id.clone(), "rm -rf /", "blocked");
        let event = rx.try_recv().expect("event should be on channel");
        assert!(matches!(event, AgentEvent::CommandDenied { .. }));
    }

    #[tokio::test]
    async fn path_denied_emits_event() {
        let (tx, mut rx) = broadcast::channel(16);
        let sink = AuditSink::new(tx);
        let run_id = RunId::new();
        sink.path_denied(run_id.clone(), "/etc/passwd", "traversal");
        let event = rx.try_recv().expect("event should be on channel");
        assert!(matches!(event, AgentEvent::PathDenied { .. }));
    }

    #[tokio::test]
    async fn overwrite_blocked_emits_event() {
        let (tx, mut rx) = broadcast::channel(16);
        let sink = AuditSink::new(tx);
        let run_id = RunId::new();
        sink.overwrite_blocked(run_id.clone(), "/etc/hosts", "protected");
        let event = rx.try_recv().expect("event should be on channel");
        assert!(matches!(event, AgentEvent::OverwriteBlocked { .. }));
    }

    #[tokio::test]
    async fn approval_denied_emits_event() {
        let (tx, mut rx) = broadcast::channel(16);
        let sink = AuditSink::new(tx);
        let run_id = RunId::new();
        sink.approval_denied(run_id.clone(), "shell", "critical", "requires approval");
        let event = rx.try_recv().expect("event should be on channel");
        assert!(matches!(event, AgentEvent::ApprovalDenied { .. }));
    }

    #[tokio::test]
    async fn approval_granted_emits_event() {
        let (tx, mut rx) = broadcast::channel(16);
        let sink = AuditSink::new(tx);
        let run_id = RunId::new();
        sink.approval_granted(run_id.clone(), "read_file", "safe");
        let event = rx.try_recv().expect("event should be on channel");
        assert!(matches!(event, AgentEvent::ApprovalGranted { .. }));
    }

    #[tokio::test]
    async fn policy_denied_emits_event() {
        let (tx, mut rx) = broadcast::channel(16);
        let sink = AuditSink::new(tx);
        sink.policy_denied("tool", "shell", Some("no-shell"), "disallowed");
        let event = rx.try_recv().expect("event should be on channel");
        assert!(matches!(event, AgentEvent::PolicyDenied { .. }));
    }
}
