//! Session state model.
//!
//! A [`Session`] is a persistent conversation context that groups multiple runs.
//! Sessions are created by the operator and can contain any number of sequential runs.

use crate::{
    error::AgentError,
    types::{AgentEvent, RunId, SessionId},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The lifecycle status of a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Session is active and can accept new runs.
    Active,
    /// Session has no current run but has had prior runs.
    Idle,
    /// Session has been explicitly ended; no new runs may be attached.
    Ended,
}

/// Lightweight key-value metadata attached to a session.
pub type SessionMeta = HashMap<String, String>;

/// A single conversation session.
///
/// Sessions are the top-level unit of persistence. Each session may contain
/// multiple sequential [`Run`]s. The session tracks its full run history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub status: SessionStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Ordered list of run IDs attached to this session.
    pub run_ids: Vec<RunId>,
    /// Optional human-readable label.
    pub label: Option<String>,
    /// Arbitrary metadata for callers.
    pub meta: SessionMeta,
}

impl Session {
    /// Create a new session and return the creation event.
    pub fn new() -> (Self, AgentEvent) {
        let id = SessionId::new();
        let now = Utc::now();
        let event = AgentEvent::SessionCreated {
            session_id: id.clone(),
            timestamp: now,
        };
        let session = Self {
            id,
            status: SessionStatus::Active,
            created_at: now,
            updated_at: now,
            run_ids: Vec::new(),
            label: None,
            meta: HashMap::new(),
        };
        (session, event)
    }

    /// Attach a run to this session and mark it updated.
    ///
    /// Returns an error if the session has ended.
    pub fn attach_run(&mut self, run_id: RunId) -> Result<(), AgentError> {
        if self.status == SessionStatus::Ended {
            return Err(AgentError::SessionNotFound(self.id.clone()));
        }
        self.run_ids.push(run_id);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Mark the session as ended and return the end event.
    pub fn end(&mut self) -> AgentEvent {
        self.status = SessionStatus::Ended;
        self.updated_at = Utc::now();
        AgentEvent::SessionEnded {
            session_id: self.id.clone(),
            timestamp: self.updated_at,
        }
    }

    /// Touch the updated_at timestamp.
    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }

    /// Most recent run ID, if any.
    pub fn current_run_id(&self) -> Option<&RunId> {
        self.run_ids.last()
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new().0
    }
}

/// In-memory session registry.
///
/// Holds all active sessions for the current process. In Phase 7 this will be
/// backed by the `session-store` crate; for now it is an in-memory map.
#[derive(Debug, Default)]
pub struct SessionManager {
    sessions: HashMap<SessionId, Session>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new session, register it, and return the creation event.
    pub fn create(&mut self) -> (SessionId, AgentEvent) {
        let (session, event) = Session::new();
        let id = session.id.clone();
        self.sessions.insert(id.clone(), session);
        (id, event)
    }

    /// Retrieve an immutable reference to a session.
    pub fn get(&self, id: &SessionId) -> Result<&Session, AgentError> {
        self.sessions
            .get(id)
            .ok_or_else(|| AgentError::SessionNotFound(id.clone()))
    }

    /// Retrieve a mutable reference to a session.
    pub fn get_mut(&mut self, id: &SessionId) -> Result<&mut Session, AgentError> {
        self.sessions
            .get_mut(id)
            .ok_or_else(|| AgentError::SessionNotFound(id.clone()))
    }

    /// List all sessions sorted by creation time.
    pub fn list(&self) -> Vec<&Session> {
        let mut sessions: Vec<&Session> = self.sessions.values().collect();
        sessions.sort_by_key(|s| s.created_at);
        sessions
    }

    /// End a session by ID and return the end event.
    pub fn end_session(&mut self, id: &SessionId) -> Result<AgentEvent, AgentError> {
        let session = self.get_mut(id)?;
        Ok(session.end())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_session_returns_event() {
        let (session, event) = Session::new();
        match event {
            AgentEvent::SessionCreated { session_id, .. } => {
                assert_eq!(session_id, session.id);
            }
            _ => panic!("expected SessionCreated event"),
        }
        assert_eq!(session.status, SessionStatus::Active);
        assert!(session.run_ids.is_empty());
    }

    #[test]
    fn session_manager_create_and_get() {
        let mut mgr = SessionManager::new();
        let (id, event) = mgr.create();
        assert!(matches!(event, AgentEvent::SessionCreated { .. }));
        let session = mgr.get(&id).unwrap();
        assert_eq!(session.id, id);
    }

    #[test]
    fn attach_run_to_session() {
        let mut mgr = SessionManager::new();
        let (id, _) = mgr.create();
        let run_id = RunId::new();
        mgr.get_mut(&id).unwrap().attach_run(run_id.clone()).unwrap();
        let session = mgr.get(&id).unwrap();
        assert_eq!(session.run_ids, vec![run_id]);
    }

    #[test]
    fn attach_run_to_ended_session_fails() {
        let mut mgr = SessionManager::new();
        let (id, _) = mgr.create();
        mgr.end_session(&id).unwrap();
        let result = mgr.get_mut(&id).unwrap().attach_run(RunId::new());
        assert!(result.is_err());
    }

    #[test]
    fn end_session_returns_event() {
        let mut mgr = SessionManager::new();
        let (id, _) = mgr.create();
        let event = mgr.end_session(&id).unwrap();
        assert!(matches!(event, AgentEvent::SessionEnded { .. }));
        assert_eq!(mgr.get(&id).unwrap().status, SessionStatus::Ended);
    }

    #[test]
    fn get_nonexistent_session_errors() {
        let mgr = SessionManager::new();
        let fake_id = SessionId::new();
        assert!(mgr.get(&fake_id).is_err());
    }
}
