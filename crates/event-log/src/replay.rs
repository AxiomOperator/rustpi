//! Replay and debug helpers for event logs.

use crate::{
    file_store::FileEventStore,
    memory_store::MemoryEventStore,
    record::EventRecord,
    EventLogError,
};
use agent_core::types::{RunId, SessionId};

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

    /// Print a human-readable summary to stderr (for debug use).
    pub fn print_summary(&self) {
        eprintln!("=== ReplayReader: {} records ===", self.records.len());
        for record in &self.records {
            let type_tag = serde_json::to_value(&record.event)
                .ok()
                .and_then(|v| v.get("type").and_then(|t| t.as_str()).map(String::from))
                .unwrap_or_else(|| "<unknown>".to_string());
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
}
