//! Replay and debug helpers for event logs.

use crate::{
    file_store::FileEventStore,
    memory_store::MemoryEventStore,
    record::EventRecord,
    EventLogError,
};
use agent_core::types::{AgentEvent, AuthFlow, RunId, SessionId};
use chrono::{DateTime, Utc};
use serde::Serialize;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A single entry in a human-readable event timeline.
#[derive(Debug, Clone, Serialize)]
pub struct TimelineEntry {
    pub seq: u64,
    pub timestamp: DateTime<Utc>,
    pub event_type: String,
    pub session_id: Option<String>,
    pub run_id: Option<String>,
    /// Human-readable one-liner description of the event.
    pub summary: String,
    /// True for events that represent a failure (run, tool, or auth).
    pub is_failure: bool,
    /// True for audit-relevant events.
    pub is_audit: bool,
}

/// A run that started but never received a completion, failure, or cancel event.
#[derive(Debug, Clone, Serialize)]
pub struct IncompleteRunRecord {
    pub run_id: String,
    pub session_id: Option<String>,
    pub started_at: DateTime<Utc>,
    pub last_event_at: DateTime<Utc>,
    pub last_event_type: String,
    /// Classification of why this run is considered incomplete.
    pub state: IncompleteRunState,
}

/// Classification for an incomplete run.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IncompleteRunState {
    /// Run started but no completion/failure/cancel found.
    Interrupted,
    /// Run started, some tool activity was observed, then cut off.
    InterruptedMidTool,
    /// Tool-level failures were recorded but no RunFailed event — process may have crashed.
    PossiblyFailed,
}

// ---------------------------------------------------------------------------
// ReplayReader
// ---------------------------------------------------------------------------

/// Replay reader for inspecting event logs.
pub struct ReplayReader {
    records: Vec<EventRecord>,
}

impl ReplayReader {
    /// Load all records from a MemoryEventStore.
    pub fn from_memory(store: &MemoryEventStore) -> Self {
        Self {
            records: store.all_records(),
        }
    }

    /// Load all records from a FileEventStore (async).
    pub async fn from_file(store: &FileEventStore) -> Result<Self, EventLogError> {
        Ok(Self {
            records: store.replay_all().await?,
        })
    }

    /// Load all records from a FileEventStore, skipping any corrupt/malformed lines.
    pub async fn from_file_tolerant(store: &FileEventStore) -> Self {
        Self {
            records: store.replay_all_tolerant().await,
        }
    }

    /// All records in sequence order.
    pub fn all(&self) -> &[EventRecord] {
        &self.records
    }

    /// Records for a specific session.
    pub fn for_session(&self, session_id: &SessionId) -> Vec<&EventRecord> {
        self.records
            .iter()
            .filter(|r| crate::memory_store::event_session_id_pub(&r.event) == Some(session_id))
            .collect()
    }

    /// Records for a specific run.
    pub fn for_run(&self, run_id: &RunId) -> Vec<&EventRecord> {
        self.records
            .iter()
            .filter(|r| crate::memory_store::event_run_id_pub(&r.event) == Some(run_id))
            .collect()
    }

    /// Only audit-relevant records.
    pub fn audit_trail(&self) -> Vec<&EventRecord> {
        self.records.iter().filter(|r| r.is_audit_relevant()).collect()
    }

    /// Records between two sequence numbers (inclusive).
    pub fn range(&self, from_seq: u64, to_seq: u64) -> Vec<&EventRecord> {
        self.records
            .iter()
            .filter(|r| r.seq >= from_seq && r.seq <= to_seq)
            .collect()
    }

    /// Build a structured timeline from all event records.
    pub fn format_timeline(&self) -> Vec<TimelineEntry> {
        self.records
            .iter()
            .map(|record| {
                let event_type = event_type_tag(&record.event);
                let session_id = crate::memory_store::event_session_id_pub(&record.event)
                    .map(|s| s.to_string());
                let run_id = crate::memory_store::event_run_id_pub(&record.event)
                    .map(|r| r.to_string());
                let summary = event_summary(&record.event);
                let is_failure = is_failure_event(&record.event);
                let is_audit = record.is_audit_relevant();
                TimelineEntry {
                    seq: record.seq,
                    timestamp: record.appended_at,
                    event_type,
                    session_id,
                    run_id,
                    summary,
                    is_failure,
                    is_audit,
                }
            })
            .collect()
    }

    /// Print the structured timeline to stdout.
    ///
    /// Format: `[seq=N] HH:MM:SS.mmm  EVENT_TYPE  summary`
    pub fn print_timeline(&self) {
        for entry in self.format_timeline() {
            println!(
                "[seq={:>4}] {}  {:<35}  {}",
                entry.seq,
                entry.timestamp.format("%H:%M:%S%.3f"),
                entry.event_type,
                entry.summary,
            );
        }
    }

    /// Return runs that started but never received a terminal event.
    ///
    /// Scans all records and returns `IncompleteRunRecord`s for runs that have
    /// a `RunStarted` but no `RunCompleted`, `RunFailed`, or `RunCancelled`.
    pub fn incomplete_runs(&self) -> Vec<IncompleteRunRecord> {
        use std::collections::HashMap;

        struct RunState {
            session_id: Option<String>,
            started_at: DateTime<Utc>,
            last_event_at: DateTime<Utc>,
            last_event_type: String,
            has_tool_events: bool,
            has_tool_failed: bool,
            completed: bool,
        }

        // Classify each event for run-tracking purposes.
        enum RunEventKind {
            Started { session_id: Option<String> },
            Completed,
            ToolFailed,
            ToolActivity,
            Other,
        }

        let mut runs: HashMap<String, RunState> = HashMap::new();

        for record in &self.records {
            let etype = event_type_tag(&record.event);

            let kind = match &record.event {
                AgentEvent::RunStarted { session_id, .. } => {
                    RunEventKind::Started { session_id: Some(session_id.to_string()) }
                }
                AgentEvent::RunCompleted { .. }
                | AgentEvent::RunFailed { .. }
                | AgentEvent::RunCancelled { .. } => RunEventKind::Completed,
                AgentEvent::ToolExecutionFailed { .. } | AgentEvent::ToolFailed { .. } => {
                    RunEventKind::ToolFailed
                }
                AgentEvent::ToolExecutionStarted { .. }
                | AgentEvent::ToolStarted { .. }
                | AgentEvent::ToolStdout { .. }
                | AgentEvent::ToolStderr { .. }
                | AgentEvent::ToolExecutionCompleted { .. }
                | AgentEvent::ToolExecutionCancelled { .. }
                | AgentEvent::ToolCompleted { .. }
                | AgentEvent::ToolCancelled { .. }
                | AgentEvent::ToolCallRequested { .. }
                | AgentEvent::ToolResultSubmitted { .. } => RunEventKind::ToolActivity,
                _ => RunEventKind::Other,
            };

            let run_id_opt = crate::memory_store::event_run_id_pub(&record.event)
                .map(|r| r.to_string());

            if let Some(rid) = run_id_opt {
                match kind {
                    RunEventKind::Started { session_id } => {
                        runs.insert(
                            rid,
                            RunState {
                                session_id,
                                started_at: record.appended_at,
                                last_event_at: record.appended_at,
                                last_event_type: etype,
                                has_tool_events: false,
                                has_tool_failed: false,
                                completed: false,
                            },
                        );
                    }
                    RunEventKind::Completed => {
                        if let Some(state) = runs.get_mut(&rid) {
                            state.completed = true;
                            state.last_event_at = record.appended_at;
                            state.last_event_type = etype;
                        }
                    }
                    RunEventKind::ToolFailed => {
                        if let Some(state) = runs.get_mut(&rid) {
                            state.has_tool_events = true;
                            state.has_tool_failed = true;
                            state.last_event_at = record.appended_at;
                            state.last_event_type = etype;
                        }
                    }
                    RunEventKind::ToolActivity => {
                        if let Some(state) = runs.get_mut(&rid) {
                            state.has_tool_events = true;
                            state.last_event_at = record.appended_at;
                            state.last_event_type = etype;
                        }
                    }
                    RunEventKind::Other => {
                        if let Some(state) = runs.get_mut(&rid) {
                            state.last_event_at = record.appended_at;
                            state.last_event_type = etype;
                        }
                    }
                }
            }
        }

        let mut incomplete: Vec<IncompleteRunRecord> = runs
            .into_iter()
            .filter(|(_, state)| !state.completed)
            .map(|(rid, state)| {
                let run_state = if state.has_tool_failed {
                    IncompleteRunState::PossiblyFailed
                } else if state.has_tool_events {
                    IncompleteRunState::InterruptedMidTool
                } else {
                    IncompleteRunState::Interrupted
                };
                IncompleteRunRecord {
                    run_id: rid,
                    session_id: state.session_id,
                    started_at: state.started_at,
                    last_event_at: state.last_event_at,
                    last_event_type: state.last_event_type,
                    state: run_state,
                }
            })
            .collect();

        // Deterministic order.
        incomplete.sort_by_key(|r| r.started_at);
        incomplete
    }

    /// Returns up to `limit` failure events (most recent first).
    ///
    /// Failure events: `RunFailed`, `ToolFailed`, `ToolExecutionFailed`,
    /// `AuthLoginFailed`, `TokenRefreshFailed`.
    pub fn recent_failures(&self, limit: usize) -> Vec<&EventRecord> {
        self.records
            .iter()
            .filter(|r| is_failure_event(&r.event))
            .rev()
            .take(limit)
            .collect()
    }

    /// Print a human-readable summary to stderr (for debug use).
    pub fn print_summary(&self) {
        eprintln!("=== ReplayReader: {} records ===", self.records.len());
        for record in &self.records {
            let type_tag = event_type_tag(&record.event);
            eprintln!(
                "  seq={:>4}  at={}  event={}{}",
                record.seq,
                record.appended_at.format("%H:%M:%S%.3f"),
                type_tag,
                if record.is_audit_relevant() { "  [AUDIT]" } else { "" },
            );
        }
        eprintln!("===");
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Extract the serde type tag string for an event (e.g. "run_started").
fn event_type_tag(event: &AgentEvent) -> String {
    serde_json::to_value(event)
        .ok()
        .and_then(|v| v.get("type").and_then(|t| t.as_str()).map(String::from))
        .unwrap_or_else(|| "<unknown>".to_string())
}

/// Build a human-readable one-line summary for an event.
fn event_summary(event: &AgentEvent) -> String {
    match event {
        AgentEvent::SessionCreated { session_id, .. } => {
            format!("Session {} created", session_id)
        }
        AgentEvent::SessionResumed { session_id, .. } => {
            format!("Session {} resumed", session_id)
        }
        AgentEvent::SessionEnded { session_id, .. } => {
            format!("Session {} ended", session_id)
        }
        AgentEvent::RunCreated { run_id, .. } => format!("Run {} created", run_id),
        AgentEvent::RunQueued { run_id, .. } => format!("Run {} queued", run_id),
        AgentEvent::RunStarted { run_id, provider, model, .. } => {
            format!("Run {} started on {}/{}", run_id, provider, model)
        }
        AgentEvent::RunCompleted { run_id, .. } => format!("Run {} completed", run_id),
        AgentEvent::RunCancelled { run_id, .. } => format!("Run {} cancelled", run_id),
        AgentEvent::RunFailed { run_id, reason, .. } => {
            format!("Run {} FAILED: {}", run_id, reason)
        }
        AgentEvent::InterruptRequested { run_id, reason, .. } => {
            format!("Interrupt requested for run {}: {}", run_id, reason)
        }
        AgentEvent::CancellationRequested { run_id, .. } => {
            format!("Cancellation requested for run {}", run_id)
        }
        AgentEvent::PromptAssembled { section_count, estimated_tokens, .. } => {
            format!("Prompt assembled: {} sections, ~{} tokens", section_count, estimated_tokens)
        }
        AgentEvent::TokenChunk { delta, .. } => {
            format!("Token chunk ({} chars)", delta.len())
        }
        AgentEvent::ToolCallRequested { call, .. } => {
            format!("Tool call requested: {} (id={})", call.name, call.id)
        }
        AgentEvent::ToolResultSubmitted { result, .. } => {
            format!("Tool result submitted: {}", result.call_id)
        }
        AgentEvent::ToolExecutionStarted { tool_name, call_id, .. } => {
            format!("Tool {} started (call {})", tool_name, call_id)
        }
        AgentEvent::ToolStdout { call_id, line, .. } => {
            format!("Tool {} stdout: {}", call_id, line)
        }
        AgentEvent::ToolStderr { call_id, line, .. } => {
            format!("Tool {} stderr: {}", call_id, line)
        }
        AgentEvent::ToolExecutionCompleted { call_id, exit_code, .. } => match exit_code {
            Some(code) => format!("Tool {} completed (exit={})", call_id, code),
            None => format!("Tool {} completed", call_id),
        },
        AgentEvent::ToolExecutionFailed { call_id, reason, .. } => {
            format!("Tool {} FAILED: {}", call_id, reason)
        }
        AgentEvent::ToolExecutionCancelled { call_id, .. } => {
            format!("Tool {} cancelled", call_id)
        }
        AgentEvent::AuthStateChanged { provider, .. } => {
            format!("Auth state changed for {}", provider)
        }
        AgentEvent::AuthLoginStarted { provider, flow, .. } => {
            format!("Auth login started for {} (flow={})", provider, auth_flow_str(flow))
        }
        AgentEvent::AuthLoginCompleted { provider, flow, .. } => {
            format!("Auth login completed for {} (flow={})", provider, auth_flow_str(flow))
        }
        AgentEvent::AuthLoginFailed { provider, reason, .. } => {
            format!("Auth login FAILED for {}: {}", provider, reason)
        }
        AgentEvent::DeviceFlowInitiated { provider, verification_uri, .. } => {
            format!("Device flow initiated for {}: {}", provider, verification_uri)
        }
        AgentEvent::DeviceCodeIssued { provider, .. } => {
            format!("Device code issued for {}", provider)
        }
        AgentEvent::TokenStored { provider, .. } => format!("Token stored for {}", provider),
        AgentEvent::TokenRefreshed { provider, .. } => {
            format!("Token refreshed for {}", provider)
        }
        AgentEvent::TokenRefreshFailed { provider, reason, .. } => {
            format!("Token refresh FAILED for {}: {}", provider, reason)
        }
        AgentEvent::AuthStateLoaded { provider, .. } => {
            format!("Auth state loaded for {}", provider)
        }
        AgentEvent::AuthStateCleared { provider, .. } => {
            format!("Auth state cleared for {}", provider)
        }
        AgentEvent::ToolStarted { tool_name, call_id, .. } => {
            format!("Tool {} started (call {})", tool_name, call_id)
        }
        AgentEvent::ToolCompleted { tool_name, call_id, exit_code, .. } => match exit_code {
            Some(code) => format!("Tool {} completed (call {}, exit={})", tool_name, call_id, code),
            None => format!("Tool {} completed (call {})", tool_name, call_id),
        },
        AgentEvent::ToolCancelled { tool_name, call_id, .. } => {
            format!("Tool {} cancelled (call {})", tool_name, call_id)
        }
        AgentEvent::ToolFailed { tool_name, call_id, reason, .. } => {
            format!("Tool {} FAILED (call {}): {}", tool_name, call_id, reason)
        }
        AgentEvent::ContextBuilt { token_count, file_count, .. } => {
            format!("Context built: {} files, {} tokens", file_count, token_count)
        }
        AgentEvent::ContextCompacted { tokens_before, tokens_after, .. } => {
            format!("Context compacted: {} → {} tokens", tokens_before, tokens_after)
        }
        AgentEvent::ApprovalDenied { tool_name, reason, .. } => {
            format!("Approval DENIED for tool '{}': {}", tool_name, reason)
        }
        AgentEvent::ApprovalGranted { tool_name, .. } => {
            format!("Approval granted for tool '{}'", tool_name)
        }
        AgentEvent::CommandDenied { command_preview, reason, .. } => {
            format!("Command DENIED '{}': {}", command_preview, reason)
        }
        AgentEvent::PathDenied { path, reason, .. } => {
            format!("Path DENIED '{}': {}", path, reason)
        }
        AgentEvent::OverwriteBlocked { path, reason, .. } => {
            format!("Overwrite BLOCKED '{}': {}", path, reason)
        }
        AgentEvent::PolicyDenied { domain, subject, reason, .. } => {
            format!("Policy DENIED [{}] '{}': {}", domain, subject, reason)
        }
        AgentEvent::DataSourceAccessed { source, detail, .. } => {
            format!("Data source accessed: {} — {}", source, detail)
        }
    }
}

/// Returns true for events that represent a failure condition.
fn is_failure_event(event: &AgentEvent) -> bool {
    matches!(
        event,
        AgentEvent::RunFailed { .. }
            | AgentEvent::ToolFailed { .. }
            | AgentEvent::ToolExecutionFailed { .. }
            | AgentEvent::AuthLoginFailed { .. }
            | AgentEvent::TokenRefreshFailed { .. }
    )
}

fn auth_flow_str(flow: &AuthFlow) -> &'static str {
    match flow {
        AuthFlow::OAuthBrowser => "oauth_browser",
        AuthFlow::DeviceCode => "device_code",
        AuthFlow::ApiKey => "api_key",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::types::{ModelId, ProviderId, RunId, SessionId};
    use crate::{AgentEvent, memory_store::MemoryEventStore, store::EventStore};
    use chrono::Utc;

    async fn build_store() -> (MemoryEventStore, SessionId, RunId) {
        let store = MemoryEventStore::new();
        let sid = SessionId::new();
        let run_id = RunId::new();

        store
            .append(&AgentEvent::SessionCreated {
                session_id: sid.clone(),
                timestamp: Utc::now(),
            })
            .await
            .unwrap();
        store
            .append(&AgentEvent::RunStarted {
                run_id: run_id.clone(),
                session_id: sid.clone(),
                provider: ProviderId::new("openai"),
                model: ModelId::new("gpt-4o"),
                timestamp: Utc::now(),
            })
            .await
            .unwrap();
        store
            .append(&AgentEvent::PromptAssembled {
                run_id: run_id.clone(),
                section_count: 2,
                estimated_tokens: 200,
                timestamp: Utc::now(),
            })
            .await
            .unwrap();
        store
            .append(&AgentEvent::RunCompleted {
                run_id: run_id.clone(),
                timestamp: Utc::now(),
            })
            .await
            .unwrap();
        store
            .append(&AgentEvent::SessionEnded {
                session_id: sid.clone(),
                timestamp: Utc::now(),
            })
            .await
            .unwrap();

        (store, sid, run_id)
    }

    #[tokio::test]
    async fn for_session_returns_correct_subset() {
        let (store, sid, _) = build_store().await;
        // Add a second session that should not appear.
        let sid2 = SessionId::new();
        store
            .append(&AgentEvent::SessionCreated {
                session_id: sid2,
                timestamp: Utc::now(),
            })
            .await
            .unwrap();

        let reader = ReplayReader::from_memory(&store);
        let records = reader.for_session(&sid);
        // SessionCreated, RunStarted (has session_id), SessionEnded = 3
        assert_eq!(records.len(), 3);
    }

    #[tokio::test]
    async fn for_run_returns_correct_subset() {
        let (store, _, run_id) = build_store().await;
        let reader = ReplayReader::from_memory(&store);
        let records = reader.for_run(&run_id);
        // RunStarted, PromptAssembled, RunCompleted = 3
        assert_eq!(records.len(), 3);
    }

    #[tokio::test]
    async fn audit_trail_returns_only_audit_records() {
        let (store, _, _) = build_store().await;
        let reader = ReplayReader::from_memory(&store);
        let audit = reader.audit_trail();
        // SessionCreated, RunStarted, RunCompleted, SessionEnded = 4
        // PromptAssembled is NOT audit-relevant
        assert_eq!(audit.len(), 4);
        for r in &audit {
            assert!(r.is_audit_relevant());
        }
    }

    #[tokio::test]
    async fn range_returns_correct_slice() {
        let (store, _, _) = build_store().await;
        let reader = ReplayReader::from_memory(&store);
        // seq 0..=4, ask for 1..=3 → 3 records
        let slice = reader.range(1, 3);
        assert_eq!(slice.len(), 3);
        assert_eq!(slice[0].seq, 1);
        assert_eq!(slice[2].seq, 3);
    }

    // -----------------------------------------------------------------------
    // Phase 12: new timeline / incomplete-run / recent-failure tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_format_timeline_produces_entries() {
        let (store, _, _) = build_store().await;
        let reader = ReplayReader::from_memory(&store);
        let timeline = reader.format_timeline();
        // build_store creates 5 events
        assert_eq!(timeline.len(), 5);
        // Verify the first entry is session_created
        assert_eq!(timeline[0].event_type, "session_created");
        // Verify the second entry is run_started
        assert_eq!(timeline[1].event_type, "run_started");
        // Verify last entry is session_ended
        assert_eq!(timeline[4].event_type, "session_ended");
    }

    #[tokio::test]
    async fn test_timeline_failure_flagged() {
        let store = MemoryEventStore::new();
        let run_id = RunId::new();
        store
            .append(&AgentEvent::RunFailed {
                run_id: run_id.clone(),
                reason: "out of tokens".to_string(),
                timestamp: Utc::now(),
            })
            .await
            .unwrap();

        let reader = ReplayReader::from_memory(&store);
        let timeline = reader.format_timeline();
        assert_eq!(timeline.len(), 1);
        assert!(timeline[0].is_failure, "RunFailed should be flagged as failure");
        assert!(timeline[0].summary.contains("FAILED"));
    }

    #[tokio::test]
    async fn test_incomplete_runs_detected() {
        let store = MemoryEventStore::new();
        let sid = SessionId::new();
        let run_id = RunId::new();

        store
            .append(&AgentEvent::RunStarted {
                run_id: run_id.clone(),
                session_id: sid.clone(),
                provider: ProviderId::new("openai"),
                model: ModelId::new("gpt-4o"),
                timestamp: Utc::now(),
            })
            .await
            .unwrap();
        // No RunCompleted/RunFailed/RunCancelled → should appear as incomplete.

        let reader = ReplayReader::from_memory(&store);
        let incomplete = reader.incomplete_runs();
        assert_eq!(incomplete.len(), 1);
        assert_eq!(incomplete[0].run_id, run_id.to_string());
        assert_eq!(incomplete[0].state, IncompleteRunState::Interrupted);
    }

    #[tokio::test]
    async fn test_incomplete_runs_none_for_complete_runs() {
        let (store, _, _) = build_store().await;
        let reader = ReplayReader::from_memory(&store);
        // build_store emits RunCompleted → no incomplete runs
        let incomplete = reader.incomplete_runs();
        assert!(incomplete.is_empty(), "completed run should not appear as incomplete");
    }

    #[tokio::test]
    async fn test_incomplete_run_mid_tool_state() {
        let store = MemoryEventStore::new();
        let sid = SessionId::new();
        let run_id = RunId::new();

        store
            .append(&AgentEvent::RunStarted {
                run_id: run_id.clone(),
                session_id: sid.clone(),
                provider: ProviderId::new("openai"),
                model: ModelId::new("gpt-4o"),
                timestamp: Utc::now(),
            })
            .await
            .unwrap();
        store
            .append(&AgentEvent::ToolExecutionStarted {
                run_id: run_id.clone(),
                call_id: "call-1".to_string(),
                tool_name: "bash".to_string(),
                timestamp: Utc::now(),
            })
            .await
            .unwrap();
        // No RunCompleted → incomplete, but has tool activity

        let reader = ReplayReader::from_memory(&store);
        let incomplete = reader.incomplete_runs();
        assert_eq!(incomplete.len(), 1);
        assert_eq!(incomplete[0].state, IncompleteRunState::InterruptedMidTool);
    }

    #[tokio::test]
    async fn test_recent_failures_returns_failures_only() {
        let store = MemoryEventStore::new();
        let sid = SessionId::new();
        let run_id = RunId::new();

        store
            .append(&AgentEvent::SessionCreated {
                session_id: sid.clone(),
                timestamp: Utc::now(),
            })
            .await
            .unwrap();
        store
            .append(&AgentEvent::RunFailed {
                run_id: run_id.clone(),
                reason: "timeout".to_string(),
                timestamp: Utc::now(),
            })
            .await
            .unwrap();
        store
            .append(&AgentEvent::SessionEnded {
                session_id: sid.clone(),
                timestamp: Utc::now(),
            })
            .await
            .unwrap();

        let reader = ReplayReader::from_memory(&store);
        let failures = reader.recent_failures(10);
        assert_eq!(failures.len(), 1);
        assert!(matches!(failures[0].event, AgentEvent::RunFailed { .. }));
    }

    #[tokio::test]
    async fn test_recent_failures_respects_limit() {
        let store = MemoryEventStore::new();
        let run_id1 = RunId::new();
        let run_id2 = RunId::new();
        let run_id3 = RunId::new();

        for rid in [&run_id1, &run_id2, &run_id3] {
            store
                .append(&AgentEvent::RunFailed {
                    run_id: rid.clone(),
                    reason: "err".to_string(),
                    timestamp: Utc::now(),
                })
                .await
                .unwrap();
        }

        let reader = ReplayReader::from_memory(&store);
        let failures = reader.recent_failures(2);
        assert_eq!(failures.len(), 2);
    }

    #[tokio::test]
    async fn test_tolerant_replay_skips_corrupt() {
        use std::path::PathBuf;
        use tokio::io::AsyncWriteExt;
        use crate::FileEventStore;

        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-artifacts");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test_tolerant_replay.log");
        let _ = std::fs::remove_file(&path);

        // Write one valid record, one corrupt line, then another valid record.
        {
            let store = FileEventStore::open(&path).await.unwrap();
            let sid = SessionId::new();
            store
                .append(&AgentEvent::SessionCreated {
                    session_id: sid.clone(),
                    timestamp: Utc::now(),
                })
                .await
                .unwrap();
        }
        // Append a corrupt line directly.
        {
            let mut file = tokio::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .await
                .unwrap();
            file.write_all(b"NOT_VALID_JSON\n").await.unwrap();
        }
        // Append another valid record.
        {
            let store = FileEventStore::open(&path).await.unwrap();
            let sid2 = SessionId::new();
            store
                .append(&AgentEvent::SessionCreated {
                    session_id: sid2,
                    timestamp: Utc::now(),
                })
                .await
                .unwrap();
        }

        let store = FileEventStore::open(&path).await.unwrap();
        // Tolerant replay should return 2 valid records, skipping the corrupt line.
        let reader = ReplayReader::from_file_tolerant(&store).await;
        assert_eq!(reader.all().len(), 2, "should skip the corrupt line");

        let _ = std::fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------
    // Phase 12 chaos / failure tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn chaos_tolerant_replay_all_corrupt_returns_empty() {
        use std::path::PathBuf;
        use tokio::io::AsyncWriteExt;
        use crate::FileEventStore;

        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-artifacts");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("chaos_all_corrupt.log");
        let _ = std::fs::remove_file(&path);

        // Write only corrupt lines — no valid JSON records.
        {
            let mut file = tokio::fs::File::create(&path).await.unwrap();
            file.write_all(b"NOT_JSON\nALSO_NOT_JSON\n{broken\n").await.unwrap();
        }

        let store = FileEventStore::open(&path).await.unwrap();
        let reader = ReplayReader::from_file_tolerant(&store).await;
        assert_eq!(reader.all().len(), 0, "all-corrupt file should return 0 records");

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn chaos_tolerant_replay_empty_file_returns_empty() {
        use std::path::PathBuf;
        use crate::FileEventStore;

        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-artifacts");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("chaos_empty_file.log");
        let _ = std::fs::remove_file(&path);

        // Create an empty file.
        tokio::fs::File::create(&path).await.unwrap();

        let store = FileEventStore::open(&path).await.unwrap();
        let reader = ReplayReader::from_file_tolerant(&store).await;
        assert_eq!(reader.all().len(), 0, "empty file should return 0 records");

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn chaos_recent_failures_mixed_success_and_failure() {
        // Store 3 successful runs and 2 failed runs; recent_failures should return only 2.
        let store = MemoryEventStore::new();
        let sid = SessionId::new();

        for _ in 0..3 {
            let run_id = RunId::new();
            store.append(&AgentEvent::RunStarted {
                run_id: run_id.clone(),
                session_id: sid.clone(),
                provider: ProviderId::new("openai"),
                model: ModelId::new("gpt-4o"),
                timestamp: Utc::now(),
            }).await.unwrap();
            store.append(&AgentEvent::RunCompleted {
                run_id,
                timestamp: Utc::now(),
            }).await.unwrap();
        }
        for _ in 0..2 {
            store.append(&AgentEvent::RunFailed {
                run_id: RunId::new(),
                reason: "rate: 429 Too Many Requests".to_string(),
                timestamp: Utc::now(),
            }).await.unwrap();
        }

        let reader = ReplayReader::from_memory(&store);
        let failures = reader.recent_failures(10);
        assert_eq!(failures.len(), 2, "should return only the 2 failed runs");
        for r in &failures {
            assert!(
                matches!(r.event, AgentEvent::RunFailed { .. }),
                "every result must be a RunFailed event"
            );
        }
    }

    #[tokio::test]
    async fn chaos_incomplete_runs_mixed_completed_and_not() {
        // Two runs complete, one never does — only the incomplete one should appear.
        let store = MemoryEventStore::new();
        let sid = SessionId::new();

        for _ in 0..2 {
            let run_id = RunId::new();
            store.append(&AgentEvent::RunStarted {
                run_id: run_id.clone(),
                session_id: sid.clone(),
                provider: ProviderId::new("openai"),
                model: ModelId::new("gpt-4o"),
                timestamp: Utc::now(),
            }).await.unwrap();
            store.append(&AgentEvent::RunCompleted {
                run_id,
                timestamp: Utc::now(),
            }).await.unwrap();
        }

        // This one starts but never completes.
        let dangling = RunId::new();
        store.append(&AgentEvent::RunStarted {
            run_id: dangling.clone(),
            session_id: sid.clone(),
            provider: ProviderId::new("openai"),
            model: ModelId::new("gpt-4o"),
            timestamp: Utc::now(),
        }).await.unwrap();

        let reader = ReplayReader::from_memory(&store);
        let incomplete = reader.incomplete_runs();
        assert_eq!(incomplete.len(), 1, "exactly one incomplete run expected");
        assert_eq!(incomplete[0].run_id, dangling.to_string());
    }

    #[tokio::test]
    async fn chaos_incomplete_run_possibly_failed_state() {
        // Run starts, ToolFailed is emitted, but no RunFailed event — process crash simulation.
        let store = MemoryEventStore::new();
        let sid = SessionId::new();
        let run_id = RunId::new();

        store.append(&AgentEvent::RunStarted {
            run_id: run_id.clone(),
            session_id: sid.clone(),
            provider: ProviderId::new("openai"),
            model: ModelId::new("gpt-4o"),
            timestamp: Utc::now(),
        }).await.unwrap();
        store.append(&AgentEvent::ToolExecutionFailed {
            run_id: run_id.clone(),
            call_id: "c-1".to_string(),
            reason: "exit 137".to_string(),
            timestamp: Utc::now(),
        }).await.unwrap();
        // No RunFailed/RunCompleted/RunCancelled follows.

        let reader = ReplayReader::from_memory(&store);
        let incomplete = reader.incomplete_runs();
        assert_eq!(incomplete.len(), 1);
        assert_eq!(incomplete[0].state, IncompleteRunState::PossiblyFailed);
    }
}
