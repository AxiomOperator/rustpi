//! In-memory append-only event store for testing and development.

use crate::{
    record::EventRecord,
    store::EventStore,
    AgentEvent, EventLogError,
};
use agent_core::types::{RunId, SessionId};
use std::sync::{Arc, Mutex};

/// Thread-safe in-memory append-only event store.
/// Stores `EventRecord`s so sequence numbers are preserved.
#[derive(Debug, Clone)]
pub struct MemoryEventStore {
    records: Arc<Mutex<Vec<EventRecord>>>,
}

impl MemoryEventStore {
    pub fn new() -> Self {
        Self {
            records: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Return all records (cloned).
    pub fn all_records(&self) -> Vec<EventRecord> {
        self.records.lock().unwrap().clone()
    }

    /// Return records matching a predicate.
    pub fn records_where<F: Fn(&EventRecord) -> bool>(&self, f: F) -> Vec<EventRecord> {
        self.records
            .lock()
            .unwrap()
            .iter()
            .filter(|r| f(r))
            .cloned()
            .collect()
    }

    /// Return audit-relevant records.
    pub fn audit_records(&self) -> Vec<EventRecord> {
        self.records_where(|r| r.is_audit_relevant())
    }

    pub fn len(&self) -> usize {
        self.records.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.lock().unwrap().is_empty()
    }
}

impl Default for MemoryEventStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract the session_id from an event, if present.
pub(crate) fn event_session_id_pub(event: &AgentEvent) -> Option<&SessionId> {
    event_session_id(event)
}

/// Extract the run_id from an event, if present.
pub(crate) fn event_run_id_pub(event: &AgentEvent) -> Option<&RunId> {
    event_run_id(event)
}

fn event_session_id(event: &AgentEvent) -> Option<&SessionId> {
    match event {
        AgentEvent::SessionCreated { session_id, .. } => Some(session_id),
        AgentEvent::SessionResumed { session_id, .. } => Some(session_id),
        AgentEvent::SessionEnded { session_id, .. } => Some(session_id),
        AgentEvent::RunCreated { session_id, .. } => Some(session_id),
        AgentEvent::RunStarted { session_id, .. } => Some(session_id),
        _ => None,
    }
}

/// Extract the run_id from an event, if present.
fn event_run_id(event: &AgentEvent) -> Option<&RunId> {
    match event {
        AgentEvent::RunCreated { run_id, .. } => Some(run_id),
        AgentEvent::RunQueued { run_id, .. } => Some(run_id),
        AgentEvent::RunStarted { run_id, .. } => Some(run_id),
        AgentEvent::RunCompleted { run_id, .. } => Some(run_id),
        AgentEvent::RunCancelled { run_id, .. } => Some(run_id),
        AgentEvent::RunFailed { run_id, .. } => Some(run_id),
        AgentEvent::InterruptRequested { run_id, .. } => Some(run_id),
        AgentEvent::CancellationRequested { run_id, .. } => Some(run_id),
        AgentEvent::PromptAssembled { run_id, .. } => Some(run_id),
        AgentEvent::TokenChunk { run_id, .. } => Some(run_id),
        AgentEvent::ToolCallRequested { run_id, .. } => Some(run_id),
        AgentEvent::ToolResultSubmitted { run_id, .. } => Some(run_id),
        AgentEvent::ToolExecutionStarted { run_id, .. } => Some(run_id),
        AgentEvent::ToolStdout { run_id, .. } => Some(run_id),
        AgentEvent::ToolStderr { run_id, .. } => Some(run_id),
        AgentEvent::ToolExecutionCompleted { run_id, .. } => Some(run_id),
        AgentEvent::ToolExecutionFailed { run_id, .. } => Some(run_id),
        AgentEvent::ToolExecutionCancelled { run_id, .. } => Some(run_id),
        AgentEvent::ContextBuilt { run_id, .. } => Some(run_id),
        AgentEvent::ContextCompacted { run_id, .. } => Some(run_id),
        AgentEvent::ApprovalDenied { run_id, .. } => Some(run_id),
        AgentEvent::ApprovalGranted { run_id, .. } => Some(run_id),
        AgentEvent::CommandDenied { run_id, .. } => Some(run_id),
        AgentEvent::PathDenied { run_id, .. } => Some(run_id),
        AgentEvent::OverwriteBlocked { run_id, .. } => Some(run_id),
        _ => None,
    }
}

#[async_trait::async_trait]
impl EventStore for MemoryEventStore {
    async fn append(&self, event: &AgentEvent) -> Result<(), EventLogError> {
        let mut records = self.records.lock().unwrap();
        let seq = records.len() as u64;
        records.push(EventRecord::new(seq, event.clone()));
        Ok(())
    }

    async fn replay_session(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<AgentEvent>, EventLogError> {
        let records = self.records.lock().unwrap();
        Ok(records
            .iter()
            .filter(|r| event_session_id(&r.event) == Some(session_id))
            .map(|r| r.event.clone())
            .collect())
    }

    async fn replay_run(&self, run_id: &RunId) -> Result<Vec<AgentEvent>, EventLogError> {
        let records = self.records.lock().unwrap();
        Ok(records
            .iter()
            .filter(|r| event_run_id(&r.event) == Some(run_id))
            .map(|r| r.event.clone())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::types::{ModelId, ProviderId, RunId, SessionId};
    use chrono::Utc;

    fn make_session_created(session_id: SessionId) -> AgentEvent {
        AgentEvent::SessionCreated {
            session_id,
            timestamp: Utc::now(),
        }
    }

    fn make_session_ended(session_id: SessionId) -> AgentEvent {
        AgentEvent::SessionEnded {
            session_id,
            timestamp: Utc::now(),
        }
    }

    fn make_run_started(run_id: RunId, session_id: SessionId) -> AgentEvent {
        AgentEvent::RunStarted {
            run_id,
            session_id,
            provider: ProviderId::new("openai"),
            model: ModelId::new("gpt-4o"),
            timestamp: Utc::now(),
        }
    }

    fn make_run_completed(run_id: RunId) -> AgentEvent {
        AgentEvent::RunCompleted {
            run_id,
            timestamp: Utc::now(),
        }
    }

    #[tokio::test]
    async fn records_are_in_sequence_order() {
        let store = MemoryEventStore::new();
        let sid = SessionId::new();
        store.append(&make_session_created(sid.clone())).await.unwrap();
        store.append(&make_session_ended(sid.clone())).await.unwrap();

        let records = store.all_records();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].seq, 0);
        assert_eq!(records[1].seq, 1);
    }

    #[tokio::test]
    async fn seq_numbers_increment() {
        let store = MemoryEventStore::new();
        for i in 0..5 {
            let sid = SessionId::new();
            store.append(&make_session_created(sid)).await.unwrap();
            let records = store.all_records();
            assert_eq!(records.last().unwrap().seq, i);
        }
    }

    #[tokio::test]
    async fn replay_session_filters_correctly() {
        let store = MemoryEventStore::new();
        let sid1 = SessionId::new();
        let sid2 = SessionId::new();

        store.append(&make_session_created(sid1.clone())).await.unwrap();
        store.append(&make_session_created(sid2.clone())).await.unwrap();
        store.append(&make_session_ended(sid1.clone())).await.unwrap();

        let events = store.replay_session(&sid1).await.unwrap();
        assert_eq!(events.len(), 2);

        let events2 = store.replay_session(&sid2).await.unwrap();
        assert_eq!(events2.len(), 1);
    }

    #[tokio::test]
    async fn replay_run_filters_correctly() {
        let store = MemoryEventStore::new();
        let sid = SessionId::new();
        let run1 = RunId::new();
        let run2 = RunId::new();

        store.append(&make_run_started(run1.clone(), sid.clone())).await.unwrap();
        store.append(&make_run_started(run2.clone(), sid.clone())).await.unwrap();
        store.append(&make_run_completed(run1.clone())).await.unwrap();

        let events = store.replay_run(&run1).await.unwrap();
        assert_eq!(events.len(), 2);

        let events2 = store.replay_run(&run2).await.unwrap();
        assert_eq!(events2.len(), 1);
    }

    #[tokio::test]
    async fn audit_records_returns_only_audit_relevant() {
        let store = MemoryEventStore::new();
        let sid = SessionId::new();
        let run_id = RunId::new();

        store.append(&make_session_created(sid.clone())).await.unwrap();
        // PromptAssembled is NOT audit-relevant
        store
            .append(&AgentEvent::PromptAssembled {
                run_id: run_id.clone(),
                section_count: 2,
                estimated_tokens: 100,
                timestamp: Utc::now(),
            })
            .await
            .unwrap();
        store.append(&make_run_started(run_id.clone(), sid.clone())).await.unwrap();

        let audit = store.audit_records();
        assert_eq!(audit.len(), 2);
    }

    #[tokio::test]
    async fn is_empty_and_len() {
        let store = MemoryEventStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);

        store.append(&make_session_created(SessionId::new())).await.unwrap();
        assert!(!store.is_empty());
        assert_eq!(store.len(), 1);
    }

    #[tokio::test]
    async fn clone_shares_backing_store() {
        let store = MemoryEventStore::new();
        let clone = store.clone();

        store.append(&make_session_created(SessionId::new())).await.unwrap();

        // Both views see the same record
        assert_eq!(clone.len(), 1);
        assert_eq!(store.len(), 1);
    }
}
