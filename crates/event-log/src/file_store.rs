//! File-backed append-only event store using JSONL format.

use crate::{
    memory_store::MemoryEventStore,
    record::EventRecord,
    store::{decode_record, encode_record, EventStore},
    AgentEvent, EventLogError,
};
use agent_core::types::{RunId, SessionId};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// File-backed append-only event store using JSONL format.
/// Each line in the file is a JSON-serialized `EventRecord`.
pub struct FileEventStore {
    path: PathBuf,
    write_lock: Arc<tokio::sync::Mutex<()>>,
    next_seq: Arc<AtomicU64>,
}

impl FileEventStore {
    /// Open or create a JSONL event log at the given path.
    /// Reads the existing file to determine the next sequence number.
    pub async fn open(path: impl Into<PathBuf>) -> Result<Self, EventLogError> {
        let path = path.into();

        // Determine next_seq by scanning existing records.
        let next_seq = if path.exists() {
            let file = tokio::fs::File::open(&path).await?;
            let reader = BufReader::new(file);
            let mut lines = reader.lines();
            let mut max_seq: Option<u64> = None;
            while let Some(line) = lines.next_line().await? {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                match decode_record(trimmed) {
                    Ok(record) => {
                        max_seq = Some(match max_seq {
                            Some(m) => m.max(record.seq),
                            None => record.seq,
                        });
                    }
                    Err(_) => {
                        // Skip corrupt lines during open; they'll surface during replay.
                    }
                }
            }
            max_seq.map(|s| s + 1).unwrap_or(0)
        } else {
            0
        };

        Ok(Self {
            path,
            write_lock: Arc::new(tokio::sync::Mutex::new(())),
            next_seq: Arc::new(AtomicU64::new(next_seq)),
        })
    }

    /// Path to the underlying log file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Return all `EventRecord`s from the file in order.
    pub async fn replay_all(&self) -> Result<Vec<EventRecord>, EventLogError> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let file = tokio::fs::File::open(&self.path).await?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();
        let mut records = Vec::new();
        let mut offset: u64 = 0;
        while let Some(line) = lines.next_line().await? {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                offset += line.len() as u64 + 1;
                continue;
            }
            match decode_record(trimmed) {
                Ok(record) => records.push(record),
                Err(_) => {
                    return Err(EventLogError::CorruptEvent {
                        offset,
                        reason: format!("failed to decode record: {trimmed}"),
                    });
                }
            }
            offset += line.len() as u64 + 1;
        }
        Ok(records)
    }
    /// Like `replay_all` but skips lines that fail to decode rather than returning an error.
    /// Useful for reading partially-corrupt or truncated log files.
    pub async fn replay_all_tolerant(&self) -> Vec<EventRecord> {
        if !self.path.exists() {
            return Vec::new();
        }
        let file = match tokio::fs::File::open(&self.path).await {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };
        let reader = BufReader::new(file);
        let mut lines = reader.lines();
        let mut records = Vec::new();
        while let Ok(Some(line)) = lines.next_line().await {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(record) = decode_record(trimmed) {
                records.push(record);
            }
            // Silently skip malformed lines.
        }
        records
    }
}

#[async_trait::async_trait]
impl EventStore for FileEventStore {
    async fn append(&self, event: &AgentEvent) -> Result<(), EventLogError> {
        let _guard = self.write_lock.lock().await;
        let seq = self.next_seq.fetch_add(1, Ordering::SeqCst);
        let record = EventRecord::new(seq, event.clone());
        let mut line = encode_record(&record)?;
        line.push('\n');

        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await
            .map_err(|e| EventLogError::WriteFailed(e.to_string()))?;

        file.write_all(line.as_bytes())
            .await
            .map_err(|e| EventLogError::WriteFailed(e.to_string()))?;

        file.flush()
            .await
            .map_err(|e| EventLogError::WriteFailed(e.to_string()))?;

        Ok(())
    }

    async fn replay_session(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<AgentEvent>, EventLogError> {
        // Delegate to a transient MemoryEventStore to reuse filtering logic.
        let records = self.replay_all().await?;
        let mem = MemoryEventStore::new();
        for record in &records {
            mem.append(&record.event).await?;
        }
        mem.replay_session(session_id).await
    }

    async fn replay_run(&self, run_id: &RunId) -> Result<Vec<AgentEvent>, EventLogError> {
        let records = self.replay_all().await?;
        let mem = MemoryEventStore::new();
        for record in &records {
            mem.append(&record.event).await?;
        }
        mem.replay_run(run_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::types::{ModelId, ProviderId, RunId, SessionId};
    use chrono::Utc;

    fn tmp_path(name: &str) -> PathBuf {
        // Use a path under the project target dir to avoid /tmp.
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test-artifacts");
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    fn cleanup(path: &Path) {
        let _ = std::fs::remove_file(path);
    }

    fn make_session_created(session_id: SessionId) -> AgentEvent {
        AgentEvent::SessionCreated {
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
    async fn append_creates_jsonl_lines() {
        let path = tmp_path("test_append_creates_jsonl.log");
        cleanup(&path);

        let store = FileEventStore::open(&path).await.unwrap();
        let sid = SessionId::new();
        store.append(&make_session_created(sid)).await.unwrap();

        let content = tokio::fs::read_to_string(&path).await.unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 1);
        // Verify it's valid JSON with a "seq" field.
        let v: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(v["seq"], 0);

        cleanup(&path);
    }

    #[tokio::test]
    async fn two_appends_create_two_lines() {
        let path = tmp_path("test_two_appends.log");
        cleanup(&path);

        let store = FileEventStore::open(&path).await.unwrap();
        let sid = SessionId::new();
        store.append(&make_session_created(sid.clone())).await.unwrap();
        store
            .append(&AgentEvent::SessionEnded {
                session_id: sid,
                timestamp: Utc::now(),
            })
            .await
            .unwrap();

        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(content.lines().count(), 2);

        cleanup(&path);
    }

    #[tokio::test]
    async fn replay_all_returns_records_in_order() {
        let path = tmp_path("test_replay_all.log");
        cleanup(&path);

        let store = FileEventStore::open(&path).await.unwrap();
        let sid = SessionId::new();
        store.append(&make_session_created(sid.clone())).await.unwrap();
        store
            .append(&AgentEvent::SessionEnded {
                session_id: sid,
                timestamp: Utc::now(),
            })
            .await
            .unwrap();

        let records = store.replay_all().await.unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].seq, 0);
        assert_eq!(records[1].seq, 1);

        cleanup(&path);
    }

    #[tokio::test]
    async fn replay_session_filters_correctly() {
        let path = tmp_path("test_file_replay_session.log");
        cleanup(&path);

        let store = FileEventStore::open(&path).await.unwrap();
        let sid1 = SessionId::new();
        let sid2 = SessionId::new();

        store.append(&make_session_created(sid1.clone())).await.unwrap();
        store.append(&make_session_created(sid2.clone())).await.unwrap();

        let events = store.replay_session(&sid1).await.unwrap();
        assert_eq!(events.len(), 1);

        let events2 = store.replay_session(&sid2).await.unwrap();
        assert_eq!(events2.len(), 1);

        cleanup(&path);
    }

    #[tokio::test]
    async fn open_existing_file_restores_sequence_numbering() {
        let path = tmp_path("test_restore_seq.log");
        cleanup(&path);

        // Write 3 records.
        {
            let store = FileEventStore::open(&path).await.unwrap();
            for _ in 0..3 {
                store.append(&make_session_created(SessionId::new())).await.unwrap();
            }
        }

        // Reopen — next_seq should be 3.
        let store2 = FileEventStore::open(&path).await.unwrap();
        let sid = SessionId::new();
        store2.append(&make_session_created(sid)).await.unwrap();

        let records = store2.replay_all().await.unwrap();
        assert_eq!(records.len(), 4);
        assert_eq!(records[3].seq, 3);

        cleanup(&path);
    }

    #[tokio::test]
    async fn replay_run_filters_correctly() {
        let path = tmp_path("test_file_replay_run.log");
        cleanup(&path);

        let store = FileEventStore::open(&path).await.unwrap();
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

        cleanup(&path);
    }
}
