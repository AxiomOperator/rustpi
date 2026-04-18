//! Sled-backed session, run, summary and memory store.
//!
//! All blocking sled operations are dispatched through `tokio::task::spawn_blocking`.
//! Sled trees are Arc-backed and cheap to clone into closures.

use crate::{
    store::{
        MemoryRecord, MemoryStore, RunRecord, RunStatus, RunStore, SessionRecord, SessionStore,
        SummaryRecord, SummaryStore,
    },
    StoreError,
};
use agent_core::types::{RunId, SessionId};
use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

const CURRENT_SCHEMA_VERSION: u32 = 1;

fn status_to_str(s: &RunStatus) -> &'static str {
    match s {
        RunStatus::Running => "running",
        RunStatus::Completed => "completed",
        RunStatus::Cancelled => "cancelled",
        RunStatus::Failed => "failed",
    }
}

fn str_to_status(s: &str) -> RunStatus {
    match s {
        "completed" => RunStatus::Completed,
        "cancelled" => RunStatus::Cancelled,
        "failed" => RunStatus::Failed,
        _ => RunStatus::Running,
    }
}

/// Sled-backed store.
pub struct SledBackend {
    db: sled::Db,
    sessions: sled::Tree,
    runs: sled::Tree,
    summaries: sled::Tree,
    memories: sled::Tree,
    meta: sled::Tree,
}

/// Internal serializable form of SessionRecord (avoids orphan issues).
#[derive(serde::Serialize, serde::Deserialize)]
struct SledSession {
    id: String,
    created_at: String,
    updated_at: String,
    summary: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SledRun {
    id: String,
    session_id: String,
    created_at: String,
    completed_at: Option<String>,
    status: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SledSummary {
    id: String,
    session_id: String,
    content: String,
    created_at: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SledMemory {
    id: String,
    session_id: Option<String>,
    content: String,
    tags: Vec<String>,
    created_at: String,
    updated_at: String,
}

impl SledBackend {
    /// Open the sled database at `path` and run migrations.
    pub fn open(path: &str) -> Result<Self, StoreError> {
        let db = sled::open(path)?;
        Self::init(db)
    }

    /// Open a temporary in-memory sled database (for tests).
    pub fn open_temporary() -> Result<Self, StoreError> {
        let db = sled::Config::new().temporary(true).open()?;
        Self::init(db)
    }

    fn init(db: sled::Db) -> Result<Self, StoreError> {
        let sessions = db.open_tree("sessions")?;
        let runs = db.open_tree("runs")?;
        let summaries = db.open_tree("summaries")?;
        let memories = db.open_tree("memories")?;
        let meta = db.open_tree("meta")?;

        // Write or verify schema version.
        let stored = meta.get(b"schema_version")?;
        match stored {
            None => {
                let bytes = CURRENT_SCHEMA_VERSION.to_be_bytes();
                meta.insert(b"schema_version", &bytes)?;
            }
            Some(bytes) => {
                let arr: [u8; 4] = bytes
                    .as_ref()
                    .try_into()
                    .map_err(|_| StoreError::Migration("bad schema_version bytes".into()))?;
                let found = u32::from_be_bytes(arr);
                if found != CURRENT_SCHEMA_VERSION {
                    return Err(StoreError::VersionMismatch {
                        found,
                        expected: CURRENT_SCHEMA_VERSION,
                    });
                }
            }
        }

        Ok(Self { db, sessions, runs, summaries, memories, meta })
    }
}

fn now_rfc() -> String {
    Utc::now().to_rfc3339()
}

fn parse_dt(s: &str) -> Result<chrono::DateTime<Utc>, StoreError> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| StoreError::Database(e.to_string()))
}

fn parse_session_id(s: &str) -> Result<SessionId, StoreError> {
    Ok(SessionId(Uuid::parse_str(s).map_err(|e| StoreError::Database(e.to_string()))?))
}

fn parse_run_id(s: &str) -> Result<RunId, StoreError> {
    Ok(RunId(Uuid::parse_str(s).map_err(|e| StoreError::Database(e.to_string()))?))
}

#[async_trait]
impl SessionStore for SledBackend {
    async fn create_session(&self) -> Result<SessionRecord, StoreError> {
        let id = SessionId::new();
        let now = now_rfc();
        let rec = SledSession {
            id: id.to_string(),
            created_at: now.clone(),
            updated_at: now.clone(),
            summary: None,
        };
        let bytes = serde_json::to_vec(&rec)?;
        let tree = self.sessions.clone();
        let key = id.to_string();
        tokio::task::spawn_blocking(move || tree.insert(key.as_bytes(), bytes))
            .await
            .map_err(|e| StoreError::Database(e.to_string()))?
            .map_err(StoreError::from)?;
        Ok(SessionRecord {
            id,
            created_at: parse_dt(&now)?,
            updated_at: parse_dt(&now)?,
            summary: None,
        })
    }

    async fn get_session(&self, id: &SessionId) -> Result<SessionRecord, StoreError> {
        let tree = self.sessions.clone();
        let key = id.to_string();
        let id_clone = id.clone();
        let result = tokio::task::spawn_blocking(move || tree.get(key.as_bytes()))
            .await
            .map_err(|e| StoreError::Database(e.to_string()))?
            .map_err(StoreError::from)?
            .ok_or_else(|| StoreError::SessionNotFound(id_clone))?;
        let rec: SledSession = serde_json::from_slice(&result)?;
        Ok(SessionRecord {
            id: parse_session_id(&rec.id)?,
            created_at: parse_dt(&rec.created_at)?,
            updated_at: parse_dt(&rec.updated_at)?,
            summary: rec.summary,
        })
    }

    async fn list_sessions(&self) -> Result<Vec<SessionRecord>, StoreError> {
        let tree = self.sessions.clone();
        let entries = tokio::task::spawn_blocking(move || {
            tree.iter()
                .map(|r| r.map(|(_, v)| v.to_vec()))
                .collect::<Result<Vec<_>, _>>()
        })
        .await
        .map_err(|e| StoreError::Database(e.to_string()))?
        .map_err(StoreError::from)?;

        entries
            .iter()
            .map(|bytes| {
                let rec: SledSession = serde_json::from_slice(bytes)?;
                Ok(SessionRecord {
                    id: parse_session_id(&rec.id)?,
                    created_at: parse_dt(&rec.created_at)?,
                    updated_at: parse_dt(&rec.updated_at)?,
                    summary: rec.summary,
                })
            })
            .collect()
    }

    async fn update_summary(&self, id: &SessionId, summary: &str) -> Result<(), StoreError> {
        let tree = self.sessions.clone();
        let key = id.to_string();
        let id_clone = id.clone();
        let summary_owned = summary.to_string();
        tokio::task::spawn_blocking(move || {
            let val = tree
                .get(key.as_bytes())?
                .ok_or_else(|| StoreError::SessionNotFound(id_clone))?;
            let mut rec: SledSession = serde_json::from_slice(&val)
                .map_err(StoreError::from)?;
            rec.summary = Some(summary_owned);
            rec.updated_at = now_rfc();
            let bytes = serde_json::to_vec(&rec).map_err(StoreError::from)?;
            tree.insert(key.as_bytes(), bytes)?;
            Ok::<_, StoreError>(())
        })
        .await
        .map_err(|e| StoreError::Database(e.to_string()))?
    }

    async fn delete_session(&self, id: &SessionId) -> Result<(), StoreError> {
        let tree = self.sessions.clone();
        let key = id.to_string();
        tokio::task::spawn_blocking(move || tree.remove(key.as_bytes()))
            .await
            .map_err(|e| StoreError::Database(e.to_string()))?
            .map_err(StoreError::from)?;
        Ok(())
    }
}

#[async_trait]
impl RunStore for SledBackend {
    async fn create_run(&self, session_id: SessionId) -> Result<RunRecord, StoreError> {
        let id = RunId::new();
        let now = now_rfc();
        let rec = SledRun {
            id: id.to_string(),
            session_id: session_id.to_string(),
            created_at: now.clone(),
            completed_at: None,
            status: status_to_str(&RunStatus::Running).to_string(),
        };
        let bytes = serde_json::to_vec(&rec)?;
        let tree = self.runs.clone();
        let key = id.to_string();
        tokio::task::spawn_blocking(move || tree.insert(key.as_bytes(), bytes))
            .await
            .map_err(|e| StoreError::Database(e.to_string()))?
            .map_err(StoreError::from)?;
        Ok(RunRecord {
            id,
            session_id,
            created_at: parse_dt(&now)?,
            completed_at: None,
            status: RunStatus::Running,
        })
    }

    async fn get_run(&self, id: &RunId) -> Result<RunRecord, StoreError> {
        let tree = self.runs.clone();
        let key = id.to_string();
        let id_clone = id.clone();
        let result = tokio::task::spawn_blocking(move || tree.get(key.as_bytes()))
            .await
            .map_err(|e| StoreError::Database(e.to_string()))?
            .map_err(StoreError::from)?
            .ok_or_else(|| StoreError::RunNotFound(id_clone))?;
        let rec: SledRun = serde_json::from_slice(&result)?;
        Ok(RunRecord {
            id: parse_run_id(&rec.id)?,
            session_id: parse_session_id(&rec.session_id)?,
            created_at: parse_dt(&rec.created_at)?,
            completed_at: rec.completed_at.as_deref().map(parse_dt).transpose()?,
            status: str_to_status(&rec.status),
        })
    }

    async fn list_runs(&self, session_id: &SessionId) -> Result<Vec<RunRecord>, StoreError> {
        let tree = self.runs.clone();
        let sid = session_id.to_string();
        let entries = tokio::task::spawn_blocking(move || {
            tree.iter()
                .map(|r| r.map(|(_, v)| v.to_vec()))
                .collect::<Result<Vec<_>, _>>()
        })
        .await
        .map_err(|e| StoreError::Database(e.to_string()))?
        .map_err(StoreError::from)?;

        entries
            .iter()
            .filter_map(|bytes| {
                let rec: SledRun = serde_json::from_slice(bytes).ok()?;
                if rec.session_id != sid {
                    return None;
                }
                Some(
                    parse_run_id(&rec.id)
                        .and_then(|id| {
                            Ok(RunRecord {
                                id,
                                session_id: parse_session_id(&rec.session_id)?,
                                created_at: parse_dt(&rec.created_at)?,
                                completed_at: rec.completed_at.as_deref().map(parse_dt).transpose()?,
                                status: str_to_status(&rec.status),
                            })
                        }),
                )
            })
            .collect()
    }

    async fn update_run_status(&self, id: &RunId, status: RunStatus) -> Result<(), StoreError> {
        let tree = self.runs.clone();
        let key = id.to_string();
        let id_clone = id.clone();
        let status_str = status_to_str(&status).to_string();
        let completed_at = match &status {
            RunStatus::Running => None,
            _ => Some(now_rfc()),
        };
        tokio::task::spawn_blocking(move || {
            let val = tree
                .get(key.as_bytes())?
                .ok_or_else(|| StoreError::RunNotFound(id_clone))?;
            let mut rec: SledRun = serde_json::from_slice(&val).map_err(StoreError::from)?;
            rec.status = status_str;
            rec.completed_at = completed_at;
            let bytes = serde_json::to_vec(&rec).map_err(StoreError::from)?;
            tree.insert(key.as_bytes(), bytes)?;
            Ok::<_, StoreError>(())
        })
        .await
        .map_err(|e| StoreError::Database(e.to_string()))?
    }
}

#[async_trait]
impl SummaryStore for SledBackend {
    async fn save_summary(&self, session_id: &SessionId, content: &str) -> Result<SummaryRecord, StoreError> {
        let id = Uuid::new_v4();
        let now = now_rfc();
        let rec = SledSummary {
            id: id.to_string(),
            session_id: session_id.to_string(),
            content: content.to_string(),
            created_at: now.clone(),
        };
        let bytes = serde_json::to_vec(&rec)?;
        let tree = self.summaries.clone();
        let key = format!("{}:{}", session_id, id);
        tokio::task::spawn_blocking(move || tree.insert(key.as_bytes(), bytes))
            .await
            .map_err(|e| StoreError::Database(e.to_string()))?
            .map_err(StoreError::from)?;
        Ok(SummaryRecord {
            id,
            session_id: session_id.clone(),
            content: content.to_string(),
            created_at: parse_dt(&now)?,
        })
    }

    async fn get_latest_summary(&self, session_id: &SessionId) -> Result<Option<SummaryRecord>, StoreError> {
        let summaries = self.list_summaries(session_id).await?;
        Ok(summaries.into_iter().next())
    }

    async fn list_summaries(&self, session_id: &SessionId) -> Result<Vec<SummaryRecord>, StoreError> {
        let tree = self.summaries.clone();
        let prefix = format!("{}:", session_id);
        let entries = tokio::task::spawn_blocking(move || {
            tree.scan_prefix(prefix.as_bytes())
                .map(|r| r.map(|(_, v)| v.to_vec()))
                .collect::<Result<Vec<_>, _>>()
        })
        .await
        .map_err(|e| StoreError::Database(e.to_string()))?
        .map_err(StoreError::from)?;

        let mut recs: Vec<SummaryRecord> = entries
            .iter()
            .map(|bytes| {
                let rec: SledSummary = serde_json::from_slice(bytes)?;
                Ok(SummaryRecord {
                    id: Uuid::parse_str(&rec.id).map_err(|e| StoreError::Database(e.to_string()))?,
                    session_id: parse_session_id(&rec.session_id)?,
                    content: rec.content,
                    created_at: parse_dt(&rec.created_at)?,
                })
            })
            .collect::<Result<Vec<_>, StoreError>>()?;
        recs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(recs)
    }
}

#[async_trait]
impl MemoryStore for SledBackend {
    async fn save_memory(
        &self,
        session_id: Option<&SessionId>,
        content: &str,
        tags: &[&str],
    ) -> Result<MemoryRecord, StoreError> {
        let id = Uuid::new_v4();
        let now = now_rfc();
        let tags_owned: Vec<String> = tags.iter().map(|s| s.to_string()).collect();
        let rec = SledMemory {
            id: id.to_string(),
            session_id: session_id.map(|s| s.to_string()),
            content: content.to_string(),
            tags: tags_owned.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        let bytes = serde_json::to_vec(&rec)?;
        let tree = self.memories.clone();
        let key = id.to_string();
        tokio::task::spawn_blocking(move || tree.insert(key.as_bytes(), bytes))
            .await
            .map_err(|e| StoreError::Database(e.to_string()))?
            .map_err(StoreError::from)?;
        Ok(MemoryRecord {
            id,
            session_id: session_id.cloned(),
            content: content.to_string(),
            tags: tags_owned,
            created_at: parse_dt(&now)?,
            updated_at: parse_dt(&now)?,
        })
    }

    async fn get_memory(&self, id: &Uuid) -> Result<MemoryRecord, StoreError> {
        let tree = self.memories.clone();
        let key = id.to_string();
        let id_clone = *id;
        let result = tokio::task::spawn_blocking(move || tree.get(key.as_bytes()))
            .await
            .map_err(|e| StoreError::Database(e.to_string()))?
            .map_err(StoreError::from)?
            .ok_or_else(|| StoreError::MemoryNotFound(id_clone.to_string()))?;
        let rec: SledMemory = serde_json::from_slice(&result)?;
        sled_memory_to_record(rec)
    }

    async fn list_memories(&self, session_id: &SessionId) -> Result<Vec<MemoryRecord>, StoreError> {
        let tree = self.memories.clone();
        let sid = session_id.to_string();
        let entries = tokio::task::spawn_blocking(move || {
            tree.iter()
                .map(|r| r.map(|(_, v)| v.to_vec()))
                .collect::<Result<Vec<_>, _>>()
        })
        .await
        .map_err(|e| StoreError::Database(e.to_string()))?
        .map_err(StoreError::from)?;

        entries
            .iter()
            .filter_map(|bytes| {
                let rec: SledMemory = serde_json::from_slice(bytes).ok()?;
                if rec.session_id.as_deref() != Some(&sid) {
                    return None;
                }
                Some(sled_memory_to_record(rec))
            })
            .collect()
    }

    /// Linear scan (O(n)) — filter memories by case-insensitive content substring.
    async fn search_memories(&self, query: &str, limit: usize) -> Result<Vec<MemoryRecord>, StoreError> {
        let tree = self.memories.clone();
        let q = query.to_lowercase();
        let entries = tokio::task::spawn_blocking(move || {
            tree.iter()
                .map(|r| r.map(|(_, v)| v.to_vec()))
                .collect::<Result<Vec<_>, _>>()
        })
        .await
        .map_err(|e| StoreError::Database(e.to_string()))?
        .map_err(StoreError::from)?;

        entries
            .iter()
            .filter_map(|bytes| {
                let rec: SledMemory = serde_json::from_slice(bytes).ok()?;
                if rec.content.to_lowercase().contains(&q) {
                    Some(sled_memory_to_record(rec))
                } else {
                    None
                }
            })
            .take(limit)
            .collect()
    }

    async fn delete_memory(&self, id: &Uuid) -> Result<(), StoreError> {
        let tree = self.memories.clone();
        let key = id.to_string();
        tokio::task::spawn_blocking(move || tree.remove(key.as_bytes()))
            .await
            .map_err(|e| StoreError::Database(e.to_string()))?
            .map_err(StoreError::from)?;
        Ok(())
    }
}

fn sled_memory_to_record(rec: SledMemory) -> Result<MemoryRecord, StoreError> {
    Ok(MemoryRecord {
        id: Uuid::parse_str(&rec.id).map_err(|e| StoreError::Database(e.to_string()))?,
        session_id: rec.session_id.as_deref().map(parse_session_id).transpose()?,
        content: rec.content,
        tags: rec.tags,
        created_at: parse_dt(&rec.created_at)?,
        updated_at: parse_dt(&rec.updated_at)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_backend() -> SledBackend {
        SledBackend::open_temporary().expect("temporary sled db")
    }

    #[tokio::test]
    async fn test_sled_create_get_session() {
        let db = tmp_backend();
        let session = db.create_session().await.unwrap();
        let fetched = db.get_session(&session.id).await.unwrap();
        assert_eq!(session.id, fetched.id);
    }

    #[tokio::test]
    async fn test_sled_create_run() {
        let db = tmp_backend();
        let session = db.create_session().await.unwrap();
        let run = db.create_run(session.id.clone()).await.unwrap();
        let runs = db.list_runs(&session.id).await.unwrap();
        assert!(runs.iter().any(|r| r.id == run.id));
    }

    #[tokio::test]
    async fn test_sled_save_summary() {
        let db = tmp_backend();
        let session = db.create_session().await.unwrap();
        db.save_summary(&session.id, "sled summary").await.unwrap();
        let latest = db.get_latest_summary(&session.id).await.unwrap();
        assert!(latest.is_some());
        assert_eq!(latest.unwrap().content, "sled summary");
    }

    #[tokio::test]
    async fn test_sled_save_and_search_memory() {
        let db = tmp_backend();
        db.save_memory(None, "sled keyword content", &["t1"]).await.unwrap();
        let results = db.search_memories("keyword", 10).await.unwrap();
        assert!(!results.is_empty());
        assert!(results[0].content.contains("keyword"));
    }
}
