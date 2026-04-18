//! Run state machine.
//!
//! A [`Run`] is a single model invocation within a session. Runs follow a strict
//! state machine with defined valid transitions. Invalid transitions return
//! [`AgentError::InvalidTransition`].
//!
//! # State machine
//!
//! ```text
//! Created ──► Queued ──► Running ──► WaitingForTool ──► Running
//!                                  └──► Completed
//!                                  └──► Failed
//!                                  └──► Cancelled
//! Created/Queued/Running/WaitingForTool ──► Cancelled  (via cancel())
//! ```

use crate::{
    error::AgentError,
    types::{AgentEvent, ModelId, ProviderId, RunId, SessionId},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

/// The lifecycle status of a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// Run has been created but not yet queued for execution.
    Created,
    /// Run is queued and waiting for a model slot.
    Queued,
    /// Run is actively being processed by the model.
    Running,
    /// Run is waiting for a tool execution to complete.
    WaitingForTool,
    /// Run completed successfully.
    Completed,
    /// Run was cancelled by the operator.
    Cancelled,
    /// Run failed due to a model or runtime error.
    Failed,
}

impl RunStatus {
    /// Returns true if this is a terminal state (no further transitions possible).
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }

    /// Returns true if cancellation is currently valid from this state.
    pub fn can_cancel(&self) -> bool {
        matches!(
            self,
            Self::Created | Self::Queued | Self::Running | Self::WaitingForTool
        )
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Created => "Created",
            Self::Queued => "Queued",
            Self::Running => "Running",
            Self::WaitingForTool => "WaitingForTool",
            Self::Completed => "Completed",
            Self::Cancelled => "Cancelled",
            Self::Failed => "Failed",
        }
    }
}

/// Parameters required to start a run.
#[derive(Debug, Clone)]
pub struct RunParams {
    pub session_id: SessionId,
    pub provider: ProviderId,
    pub model: ModelId,
}

/// A single model run within a session.
///
/// Holds the full lifecycle state and a [`CancellationToken`] that can be
/// triggered to cancel all in-flight async work associated with this run.
#[derive(Debug)]
pub struct Run {
    pub id: RunId,
    pub session_id: SessionId,
    pub status: RunStatus,
    pub provider: Option<ProviderId>,
    pub model: Option<ModelId>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    /// Token that is cancelled when this run is cancelled or fails.
    pub cancel_token: CancellationToken,
    /// Human-readable failure reason, set on `Failed` status.
    pub failure_reason: Option<String>,
}

impl Run {
    /// Create a new run in the `Created` state and return its creation event.
    pub fn new(session_id: SessionId) -> (Self, AgentEvent) {
        let id = RunId::new();
        let now = Utc::now();
        let event = AgentEvent::RunCreated {
            run_id: id.clone(),
            session_id: session_id.clone(),
            timestamp: now,
        };
        let run = Self {
            id,
            session_id,
            status: RunStatus::Created,
            provider: None,
            model: None,
            created_at: now,
            started_at: None,
            completed_at: None,
            cancel_token: CancellationToken::new(),
            failure_reason: None,
        };
        (run, event)
    }

    /// Transition: Created → Queued.
    pub fn queue(&mut self) -> Result<AgentEvent, AgentError> {
        self.require_status(&RunStatus::Created, &RunStatus::Queued)?;
        self.status = RunStatus::Queued;
        Ok(AgentEvent::RunQueued {
            run_id: self.id.clone(),
            timestamp: Utc::now(),
        })
    }

    /// Transition: Queued → Running.
    pub fn start(&mut self, params: RunParams) -> Result<AgentEvent, AgentError> {
        self.require_status(&RunStatus::Queued, &RunStatus::Running)?;
        let now = Utc::now();
        self.status = RunStatus::Running;
        self.provider = Some(params.provider.clone());
        self.model = Some(params.model.clone());
        self.started_at = Some(now);
        Ok(AgentEvent::RunStarted {
            run_id: self.id.clone(),
            session_id: self.session_id.clone(),
            provider: params.provider,
            model: params.model,
            timestamp: now,
        })
    }

    /// Transition: Running → WaitingForTool.
    pub fn wait_for_tool(&mut self) -> Result<(), AgentError> {
        self.require_status(&RunStatus::Running, &RunStatus::WaitingForTool)?;
        self.status = RunStatus::WaitingForTool;
        Ok(())
    }

    /// Transition: WaitingForTool → Running.
    pub fn resume_from_tool(&mut self) -> Result<(), AgentError> {
        self.require_status(&RunStatus::WaitingForTool, &RunStatus::Running)?;
        self.status = RunStatus::Running;
        Ok(())
    }

    /// Transition: Running | WaitingForTool → Completed.
    pub fn complete(&mut self) -> Result<AgentEvent, AgentError> {
        if self.status == RunStatus::Completed {
            return Err(AgentError::AlreadyCompleted(self.id.clone()));
        }
        if !matches!(self.status, RunStatus::Running | RunStatus::WaitingForTool) {
            return Err(AgentError::InvalidTransition {
                from: self.status.name().to_string(),
                to: "Completed".to_string(),
            });
        }
        let now = Utc::now();
        self.status = RunStatus::Completed;
        self.completed_at = Some(now);
        Ok(AgentEvent::RunCompleted {
            run_id: self.id.clone(),
            timestamp: now,
        })
    }

    /// Transition: any non-terminal state → Failed.
    pub fn fail(&mut self, reason: impl Into<String>) -> Result<AgentEvent, AgentError> {
        if self.status.is_terminal() {
            return Err(AgentError::InvalidTransition {
                from: self.status.name().to_string(),
                to: "Failed".to_string(),
            });
        }
        let reason = reason.into();
        let now = Utc::now();
        self.status = RunStatus::Failed;
        self.failure_reason = Some(reason.clone());
        self.completed_at = Some(now);
        self.cancel_token.cancel();
        Ok(AgentEvent::RunFailed {
            run_id: self.id.clone(),
            reason,
            timestamp: now,
        })
    }

    /// Cancel this run from any non-terminal state.
    ///
    /// Triggers the [`CancellationToken`] so that any awaiting tasks can observe
    /// the cancellation via `cancel_token.cancelled().await`.
    pub fn cancel(&mut self) -> Result<(AgentEvent, AgentEvent), AgentError> {
        if self.status == RunStatus::Cancelled {
            return Err(AgentError::AlreadyCancelled(self.id.clone()));
        }
        if !self.status.can_cancel() {
            return Err(AgentError::InvalidTransition {
                from: self.status.name().to_string(),
                to: "Cancelled".to_string(),
            });
        }
        let now = Utc::now();
        // Emit CancellationRequested first, then RunCancelled.
        let requested = AgentEvent::CancellationRequested {
            run_id: self.id.clone(),
            timestamp: now,
        };
        self.cancel_token.cancel();
        self.status = RunStatus::Cancelled;
        self.completed_at = Some(now);
        let cancelled = AgentEvent::RunCancelled {
            run_id: self.id.clone(),
            timestamp: now,
        };
        Ok((requested, cancelled))
    }

    /// Request an interrupt without changing run status.
    ///
    /// Emits [`AgentEvent::InterruptRequested`]. The caller is responsible for
    /// deciding whether to cancel or pause after receiving this event.
    pub fn interrupt(&self, reason: impl Into<String>) -> AgentEvent {
        AgentEvent::InterruptRequested {
            run_id: self.id.clone(),
            reason: reason.into(),
            timestamp: Utc::now(),
        }
    }

    // --- Private helpers ---

    fn require_status(
        &self,
        expected: &RunStatus,
        target: &RunStatus,
    ) -> Result<(), AgentError> {
        if &self.status != expected {
            return Err(AgentError::InvalidTransition {
                from: self.status.name().to_string(),
                to: target.name().to_string(),
            });
        }
        Ok(())
    }
}

/// In-memory run registry.
///
/// Tracks all runs for the current process. In Phase 7, runs will additionally
/// be persisted through the `session-store` crate.
#[derive(Debug, Default)]
pub struct RunManager {
    runs: std::collections::HashMap<RunId, Run>,
}

impl RunManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create and register a new run, returning its creation event.
    pub fn create(&mut self, session_id: SessionId) -> (RunId, AgentEvent) {
        let (run, event) = Run::new(session_id);
        let id = run.id.clone();
        self.runs.insert(id.clone(), run);
        (id, event)
    }

    pub fn get(&self, id: &RunId) -> Result<&Run, AgentError> {
        self.runs
            .get(id)
            .ok_or_else(|| AgentError::RunNotFound(id.clone()))
    }

    pub fn get_mut(&mut self, id: &RunId) -> Result<&mut Run, AgentError> {
        self.runs
            .get_mut(id)
            .ok_or_else(|| AgentError::RunNotFound(id.clone()))
    }

    /// All runs for a given session, sorted by creation time.
    pub fn runs_for_session(&self, session_id: &SessionId) -> Vec<&Run> {
        let mut runs: Vec<&Run> = self
            .runs
            .values()
            .filter(|r| &r.session_id == session_id)
            .collect();
        runs.sort_by_key(|r| r.created_at);
        runs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ModelId, ProviderId};

    fn make_params() -> RunParams {
        RunParams {
            session_id: SessionId::new(),
            provider: ProviderId::new("test-provider"),
            model: ModelId::new("test-model"),
        }
    }

    #[test]
    fn run_created_event() {
        let session_id = SessionId::new();
        let (run, event) = Run::new(session_id.clone());
        assert_eq!(run.status, RunStatus::Created);
        match event {
            AgentEvent::RunCreated { run_id, session_id: sid, .. } => {
                assert_eq!(run_id, run.id);
                assert_eq!(sid, session_id);
            }
            _ => panic!("expected RunCreated"),
        }
    }

    #[test]
    fn full_happy_path() {
        let session_id = SessionId::new();
        let (mut run, _) = Run::new(session_id.clone());

        let ev = run.queue().unwrap();
        assert!(matches!(ev, AgentEvent::RunQueued { .. }));
        assert_eq!(run.status, RunStatus::Queued);

        let params = RunParams {
            session_id,
            provider: ProviderId::new("openai"),
            model: ModelId::new("gpt-4o"),
        };
        let ev = run.start(params).unwrap();
        assert!(matches!(ev, AgentEvent::RunStarted { .. }));
        assert_eq!(run.status, RunStatus::Running);
        assert!(run.started_at.is_some());

        let ev = run.complete().unwrap();
        assert!(matches!(ev, AgentEvent::RunCompleted { .. }));
        assert_eq!(run.status, RunStatus::Completed);
        assert!(run.completed_at.is_some());
        assert!(run.status.is_terminal());
    }

    #[test]
    fn cancellation_flow() {
        let session_id = SessionId::new();
        let (mut run, _) = Run::new(session_id);
        run.queue().unwrap();
        run.start(make_params()).unwrap();
        assert!(!run.cancel_token.is_cancelled());

        let (req_ev, cancel_ev) = run.cancel().unwrap();
        assert!(matches!(req_ev, AgentEvent::CancellationRequested { .. }));
        assert!(matches!(cancel_ev, AgentEvent::RunCancelled { .. }));
        assert_eq!(run.status, RunStatus::Cancelled);
        assert!(run.cancel_token.is_cancelled());
    }

    #[test]
    fn double_cancel_errors() {
        let session_id = SessionId::new();
        let (mut run, _) = Run::new(session_id);
        run.queue().unwrap();
        run.cancel().unwrap();
        assert!(matches!(
            run.cancel().unwrap_err(),
            AgentError::AlreadyCancelled(_)
        ));
    }

    #[test]
    fn cancel_completed_run_errors() {
        let session_id = SessionId::new();
        let (mut run, _) = Run::new(session_id);
        run.queue().unwrap();
        run.start(make_params()).unwrap();
        run.complete().unwrap();
        assert!(matches!(
            run.cancel().unwrap_err(),
            AgentError::InvalidTransition { .. }
        ));
    }

    #[test]
    fn invalid_transition_rejected() {
        let session_id = SessionId::new();
        // Cannot start a run that is in Created state (must queue first).
        let (mut run, _) = Run::new(session_id.clone());
        assert!(matches!(
            run.start(make_params()).unwrap_err(),
            AgentError::InvalidTransition { .. }
        ));
    }

    #[test]
    fn wait_for_tool_and_resume() {
        let session_id = SessionId::new();
        let (mut run, _) = Run::new(session_id);
        run.queue().unwrap();
        run.start(make_params()).unwrap();
        run.wait_for_tool().unwrap();
        assert_eq!(run.status, RunStatus::WaitingForTool);
        run.resume_from_tool().unwrap();
        assert_eq!(run.status, RunStatus::Running);
    }

    #[test]
    fn fail_run() {
        let session_id = SessionId::new();
        let (mut run, _) = Run::new(session_id);
        run.queue().unwrap();
        run.start(make_params()).unwrap();
        let ev = run.fail("something went wrong").unwrap();
        assert!(matches!(ev, AgentEvent::RunFailed { .. }));
        assert_eq!(run.status, RunStatus::Failed);
        assert!(run.cancel_token.is_cancelled());
    }

    #[test]
    fn interrupt_does_not_change_status() {
        let session_id = SessionId::new();
        let (mut run, _) = Run::new(session_id);
        run.queue().unwrap();
        run.start(make_params()).unwrap();
        let ev = run.interrupt("user pressed ctrl-c");
        assert!(matches!(ev, AgentEvent::InterruptRequested { .. }));
        // Status unchanged.
        assert_eq!(run.status, RunStatus::Running);
    }

    #[test]
    fn run_manager_create_and_get() {
        let mut mgr = RunManager::new();
        let session_id = SessionId::new();
        let (id, event) = mgr.create(session_id.clone());
        assert!(matches!(event, AgentEvent::RunCreated { .. }));
        let run = mgr.get(&id).unwrap();
        assert_eq!(run.id, id);
    }

    #[test]
    fn run_manager_runs_for_session() {
        let mut mgr = RunManager::new();
        let sid = SessionId::new();
        let (id1, _) = mgr.create(sid.clone());
        let (id2, _) = mgr.create(sid.clone());
        let other_sid = SessionId::new();
        let (_, _) = mgr.create(other_sid);

        let runs = mgr.runs_for_session(&sid);
        assert_eq!(runs.len(), 2);
        let ids: Vec<_> = runs.iter().map(|r| &r.id).collect();
        assert!(ids.contains(&&id1));
        assert!(ids.contains(&&id2));
    }
}
