//! In-process memory record type for memory-sync.

use agent_core::types::SessionId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type MemoryId = Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub id: MemoryId,
    pub session_id: Option<SessionId>,
    pub content: String,
    pub tags: Vec<String>,
    pub embedding: Option<Vec<f32>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl MemoryRecord {
    pub fn new(content: impl Into<String>, tags: Vec<String>, session_id: Option<SessionId>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            session_id,
            content: content.into(),
            tags,
            embedding: None,
            created_at: now,
            updated_at: now,
        }
    }
}
