//! Store traits for sessions and runs.

use crate::StoreError;
use agent_core::types::{RunId, SessionId};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Stored session record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub id: SessionId,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub summary: Option<String>,
}

/// Stored run record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    pub id: RunId,
    pub session_id: SessionId,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub status: RunStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    Completed,
    Cancelled,
    Failed,
}

/// Session persistence trait.
#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn create_session(&self) -> Result<SessionRecord, StoreError>;
    async fn get_session(&self, id: &SessionId) -> Result<SessionRecord, StoreError>;
    async fn list_sessions(&self) -> Result<Vec<SessionRecord>, StoreError>;
    async fn update_summary(&self, id: &SessionId, summary: &str) -> Result<(), StoreError>;
    async fn delete_session(&self, id: &SessionId) -> Result<(), StoreError>;
}

/// Run persistence trait.
#[async_trait]
pub trait RunStore: Send + Sync {
    async fn create_run(
        &self,
        session_id: SessionId,
    ) -> Result<RunRecord, StoreError>;
    async fn get_run(&self, id: &RunId) -> Result<RunRecord, StoreError>;
    async fn list_runs(&self, session_id: &SessionId) -> Result<Vec<RunRecord>, StoreError>;
    async fn update_run_status(&self, id: &RunId, status: RunStatus) -> Result<(), StoreError>;
}
