//! SQLite-backed session, run, summary and memory store.
//!
//! Uses `sqlx::SqlitePool` with runtime (non-macro) queries.

use crate::{
    store::{
        MemoryRecord, MemoryStore, RunRecord, RunStatus, RunStore, SessionRecord, SessionStore,
        SummaryRecord, SummaryStore,
    },
    StoreError,
};
use agent_core::types::{RunId, SessionId};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

const CURRENT_SCHEMA_VERSION: u32 = 1;

/// SQLite-backed store implementing all four store traits.
pub struct SqliteBackend {
    pool: SqlitePool,
}

impl SqliteBackend {
    /// Open (or create) the SQLite database at `path` and run migrations.
    ///
    /// Use `"sqlite::memory:"` for an in-memory database.
    pub async fn connect(path: &str) -> Result<Self, StoreError> {
        let pool = SqlitePool::connect(path)
            .await
            .map_err(|e| StoreError::Connection(e.to_string()))?;
        let backend = Self { pool };
        backend.migrate().await?;
        Ok(backend)
    }

    async fn migrate(&self) -> Result<(), StoreError> {
        let stmts = [
            "CREATE TABLE IF NOT EXISTS sessions (id TEXT PRIMARY KEY, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, summary TEXT)",
            "CREATE TABLE IF NOT EXISTS runs (id TEXT PRIMARY KEY, session_id TEXT NOT NULL, created_at TEXT NOT NULL, completed_at TEXT, status TEXT NOT NULL)",
            "CREATE TABLE IF NOT EXISTS summaries (id TEXT PRIMARY KEY, session_id TEXT NOT NULL, content TEXT NOT NULL, created_at TEXT NOT NULL)",
            "CREATE TABLE IF NOT EXISTS memories (id TEXT PRIMARY KEY, session_id TEXT, content TEXT NOT NULL, tags TEXT NOT NULL DEFAULT '[]', created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
            "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL)",
            "INSERT OR IGNORE INTO schema_version (version) VALUES (1)",
        ];
        for stmt in &stmts {
            sqlx::query(stmt)
                .execute(&self.pool)
                .await
                .map_err(|e| StoreError::Migration(e.to_string()))?;
        }
        // Validate schema version
        let row = sqlx::query("SELECT version FROM schema_version LIMIT 1")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| StoreError::Migration(e.to_string()))?;
        let found: i64 = row.get("version");
        if found as u32 != CURRENT_SCHEMA_VERSION {
            return Err(StoreError::VersionMismatch {
                found: found as u32,
                expected: CURRENT_SCHEMA_VERSION,
            });
        }
        Ok(())
    }
}

fn now_str() -> String {
    Utc::now().to_rfc3339()
}

fn parse_dt(s: &str) -> Result<DateTime<Utc>, StoreError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| StoreError::Database(e.to_string()))
}

fn parse_session_id(s: &str) -> Result<SessionId, StoreError> {
    let uuid = Uuid::parse_str(s).map_err(|e| StoreError::Database(e.to_string()))?;
    Ok(SessionId(uuid))
}

fn parse_run_id(s: &str) -> Result<RunId, StoreError> {
    let uuid = Uuid::parse_str(s).map_err(|e| StoreError::Database(e.to_string()))?;
    Ok(RunId(uuid))
}

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

#[async_trait]
impl SessionStore for SqliteBackend {
    async fn create_session(&self) -> Result<SessionRecord, StoreError> {
        let id = SessionId::new();
        let now = now_str();
        sqlx::query("INSERT INTO sessions (id, created_at, updated_at, summary) VALUES (?, ?, ?, NULL)")
            .bind(id.to_string())
            .bind(&now)
            .bind(&now)
            .execute(&self.pool)
            .await?;
        Ok(SessionRecord {
            id,
            created_at: parse_dt(&now)?,
            updated_at: parse_dt(&now)?,
            summary: None,
        })
    }

    async fn get_session(&self, id: &SessionId) -> Result<SessionRecord, StoreError> {
        let row = sqlx::query("SELECT id, created_at, updated_at, summary FROM sessions WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| StoreError::SessionNotFound(id.clone()))?;
        let id_str: String = row.get("id");
        let created_at: String = row.get("created_at");
        let updated_at: String = row.get("updated_at");
        let summary: Option<String> = row.get("summary");
        Ok(SessionRecord {
            id: parse_session_id(&id_str)?,
            created_at: parse_dt(&created_at)?,
            updated_at: parse_dt(&updated_at)?,
            summary,
        })
    }

    async fn list_sessions(&self) -> Result<Vec<SessionRecord>, StoreError> {
        let rows = sqlx::query("SELECT id, created_at, updated_at, summary FROM sessions ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await?;
        rows.iter()
            .map(|row| {
                let id_str: String = row.get("id");
                let created_at: String = row.get("created_at");
                let updated_at: String = row.get("updated_at");
                let summary: Option<String> = row.get("summary");
                Ok(SessionRecord {
                    id: parse_session_id(&id_str)?,
                    created_at: parse_dt(&created_at)?,
                    updated_at: parse_dt(&updated_at)?,
                    summary,
                })
            })
            .collect()
    }

    async fn update_summary(&self, id: &SessionId, summary: &str) -> Result<(), StoreError> {
        let now = now_str();
        sqlx::query("UPDATE sessions SET summary = ?, updated_at = ? WHERE id = ?")
            .bind(summary)
            .bind(&now)
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn delete_session(&self, id: &SessionId) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[async_trait]
impl RunStore for SqliteBackend {
    async fn create_run(&self, session_id: SessionId) -> Result<RunRecord, StoreError> {
        let id = RunId::new();
        let now = now_str();
        sqlx::query(
            "INSERT INTO runs (id, session_id, created_at, completed_at, status) VALUES (?, ?, ?, NULL, ?)",
        )
        .bind(id.to_string())
        .bind(session_id.to_string())
        .bind(&now)
        .bind(status_to_str(&RunStatus::Running))
        .execute(&self.pool)
        .await?;
        Ok(RunRecord {
            id,
            session_id,
            created_at: parse_dt(&now)?,
            completed_at: None,
            status: RunStatus::Running,
        })
    }

    async fn get_run(&self, id: &RunId) -> Result<RunRecord, StoreError> {
        let row =
            sqlx::query("SELECT id, session_id, created_at, completed_at, status FROM runs WHERE id = ?")
                .bind(id.to_string())
                .fetch_optional(&self.pool)
                .await?
                .ok_or_else(|| StoreError::RunNotFound(id.clone()))?;
        let id_str: String = row.get("id");
        let session_id_str: String = row.get("session_id");
        let created_at: String = row.get("created_at");
        let completed_at: Option<String> = row.get("completed_at");
        let status_str: String = row.get("status");
        Ok(RunRecord {
            id: parse_run_id(&id_str)?,
            session_id: parse_session_id(&session_id_str)?,
            created_at: parse_dt(&created_at)?,
            completed_at: completed_at.as_deref().map(parse_dt).transpose()?,
            status: str_to_status(&status_str),
        })
    }

    async fn list_runs(&self, session_id: &SessionId) -> Result<Vec<RunRecord>, StoreError> {
        let rows = sqlx::query(
            "SELECT id, session_id, created_at, completed_at, status FROM runs WHERE session_id = ? ORDER BY created_at DESC",
        )
        .bind(session_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|row| {
                let id_str: String = row.get("id");
                let session_id_str: String = row.get("session_id");
                let created_at: String = row.get("created_at");
                let completed_at: Option<String> = row.get("completed_at");
                let status_str: String = row.get("status");
                Ok(RunRecord {
                    id: parse_run_id(&id_str)?,
                    session_id: parse_session_id(&session_id_str)?,
                    created_at: parse_dt(&created_at)?,
                    completed_at: completed_at.as_deref().map(parse_dt).transpose()?,
                    status: str_to_status(&status_str),
                })
            })
            .collect()
    }

    async fn update_run_status(&self, id: &RunId, status: RunStatus) -> Result<(), StoreError> {
        let completed_at = match &status {
            RunStatus::Running => None,
            _ => Some(now_str()),
        };
        sqlx::query("UPDATE runs SET status = ?, completed_at = ? WHERE id = ?")
            .bind(status_to_str(&status))
            .bind(completed_at)
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[async_trait]
impl SummaryStore for SqliteBackend {
    async fn save_summary(&self, session_id: &SessionId, content: &str) -> Result<SummaryRecord, StoreError> {
        let id = Uuid::new_v4();
        let now = now_str();
        sqlx::query("INSERT INTO summaries (id, session_id, content, created_at) VALUES (?, ?, ?, ?)")
            .bind(id.to_string())
            .bind(session_id.to_string())
            .bind(content)
            .bind(&now)
            .execute(&self.pool)
            .await?;
        Ok(SummaryRecord {
            id,
            session_id: session_id.clone(),
            content: content.to_string(),
            created_at: parse_dt(&now)?,
        })
    }

    async fn get_latest_summary(&self, session_id: &SessionId) -> Result<Option<SummaryRecord>, StoreError> {
        let row = sqlx::query(
            "SELECT id, session_id, content, created_at FROM summaries WHERE session_id = ? ORDER BY created_at DESC LIMIT 1",
        )
        .bind(session_id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| {
            let id_str: String = r.get("id");
            let sid_str: String = r.get("session_id");
            let content: String = r.get("content");
            let created_at: String = r.get("created_at");
            Ok(SummaryRecord {
                id: Uuid::parse_str(&id_str).map_err(|e| StoreError::Database(e.to_string()))?,
                session_id: parse_session_id(&sid_str)?,
                content,
                created_at: parse_dt(&created_at)?,
            })
        })
        .transpose()
    }

    async fn list_summaries(&self, session_id: &SessionId) -> Result<Vec<SummaryRecord>, StoreError> {
        let rows = sqlx::query(
            "SELECT id, session_id, content, created_at FROM summaries WHERE session_id = ? ORDER BY created_at DESC",
        )
        .bind(session_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|r| {
                let id_str: String = r.get("id");
                let sid_str: String = r.get("session_id");
                let content: String = r.get("content");
                let created_at: String = r.get("created_at");
                Ok(SummaryRecord {
                    id: Uuid::parse_str(&id_str).map_err(|e| StoreError::Database(e.to_string()))?,
                    session_id: parse_session_id(&sid_str)?,
                    content,
                    created_at: parse_dt(&created_at)?,
                })
            })
            .collect()
    }
}

#[async_trait]
impl MemoryStore for SqliteBackend {
    async fn save_memory(
        &self,
        session_id: Option<&SessionId>,
        content: &str,
        tags: &[&str],
    ) -> Result<MemoryRecord, StoreError> {
        let id = Uuid::new_v4();
        let now = now_str();
        let tags_owned: Vec<String> = tags.iter().map(|s| s.to_string()).collect();
        let tags_json = serde_json::to_string(&tags_owned)?;
        let sid_str = session_id.map(|s| s.to_string());
        sqlx::query(
            "INSERT INTO memories (id, session_id, content, tags, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(&sid_str)
        .bind(content)
        .bind(&tags_json)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;
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
        let row = sqlx::query(
            "SELECT id, session_id, content, tags, created_at, updated_at FROM memories WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StoreError::MemoryNotFound(id.to_string()))?;
        parse_memory_row(&row)
    }

    async fn list_memories(&self, session_id: &SessionId) -> Result<Vec<MemoryRecord>, StoreError> {
        let rows = sqlx::query(
            "SELECT id, session_id, content, tags, created_at, updated_at FROM memories WHERE session_id = ? ORDER BY created_at DESC",
        )
        .bind(session_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(parse_memory_row).collect()
    }

    async fn search_memories(&self, query: &str, limit: usize) -> Result<Vec<MemoryRecord>, StoreError> {
        let rows = sqlx::query(
            "SELECT id, session_id, content, tags, created_at, updated_at FROM memories WHERE content LIKE '%' || ? || '%' ORDER BY created_at DESC LIMIT ?",
        )
        .bind(query)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(parse_memory_row).collect()
    }

    async fn delete_memory(&self, id: &Uuid) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM memories WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

fn parse_memory_row(row: &sqlx::sqlite::SqliteRow) -> Result<MemoryRecord, StoreError> {
    let id_str: String = row.get("id");
    let sid_str: Option<String> = row.get("session_id");
    let content: String = row.get("content");
    let tags_json: String = row.get("tags");
    let created_at: String = row.get("created_at");
    let updated_at: String = row.get("updated_at");
    let tags: Vec<String> = serde_json::from_str(&tags_json)?;
    Ok(MemoryRecord {
        id: Uuid::parse_str(&id_str).map_err(|e| StoreError::Database(e.to_string()))?,
        session_id: sid_str.as_deref().map(parse_session_id).transpose()?,
        content,
        tags,
        created_at: parse_dt(&created_at)?,
        updated_at: parse_dt(&updated_at)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> SqliteBackend {
        SqliteBackend::connect("sqlite::memory:").await.expect("in-memory db")
    }

    #[tokio::test]
    async fn test_create_get_session() {
        let db = mem_db().await;
        let session = db.create_session().await.unwrap();
        let fetched = db.get_session(&session.id).await.unwrap();
        assert_eq!(session.id, fetched.id);
    }

    #[tokio::test]
    async fn test_create_run() {
        let db = mem_db().await;
        let session = db.create_session().await.unwrap();
        let run = db.create_run(session.id.clone()).await.unwrap();
        let runs = db.list_runs(&session.id).await.unwrap();
        assert!(runs.iter().any(|r| r.id == run.id));
    }

    #[tokio::test]
    async fn test_update_run_status() {
        let db = mem_db().await;
        let session = db.create_session().await.unwrap();
        let run = db.create_run(session.id.clone()).await.unwrap();
        db.update_run_status(&run.id, RunStatus::Completed).await.unwrap();
        let fetched = db.get_run(&run.id).await.unwrap();
        assert!(matches!(fetched.status, RunStatus::Completed));
    }

    #[tokio::test]
    async fn test_save_and_get_summary() {
        let db = mem_db().await;
        let session = db.create_session().await.unwrap();
        db.save_summary(&session.id, "summary content").await.unwrap();
        let latest = db.get_latest_summary(&session.id).await.unwrap();
        assert!(latest.is_some());
        assert_eq!(latest.unwrap().content, "summary content");
    }

    #[tokio::test]
    async fn test_save_and_search_memory() {
        let db = mem_db().await;
        db.save_memory(None, "hello world content", &["tag1"]).await.unwrap();
        let results = db.search_memories("world", 10).await.unwrap();
        assert!(!results.is_empty());
        assert!(results[0].content.contains("world"));
    }

    #[tokio::test]
    async fn test_schema_version() {
        let db = mem_db().await;
        let row = sqlx::query("SELECT version FROM schema_version LIMIT 1")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        let version: i64 = row.get("version");
        assert_eq!(version as u32, CURRENT_SCHEMA_VERSION);
    }
}
