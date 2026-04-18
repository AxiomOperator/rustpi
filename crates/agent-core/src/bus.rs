//! In-memory append-only event bus.
//!
//! [`EventBus`] serves two purposes:
//! 1. **Durable record**: an append-only `Vec<AgentEvent>` that preserves insertion order
//!    and can be replayed for testing or crash recovery.
//! 2. **Live fan-out**: a `tokio::sync::broadcast` channel that delivers events to any
//!    number of concurrent subscribers (TUI, RPC, tests).
//!
//! In Phase 2 the `event-log` crate will provide a persistent backend; this implementation
//! is the in-process layer that will remain as the live fan-out mechanism.

use crate::types::{AgentEvent, RunId, SessionId};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

/// Default broadcast channel capacity.
///
/// Slow receivers will begin to lag if they fall more than `CHANNEL_CAPACITY`
/// events behind. The bus does not block on slow receivers — it relies on the
/// durable append log for replay.
const CHANNEL_CAPACITY: usize = 256;

/// The append-only, fan-out runtime event bus.
///
/// Clone cheaply — all clones share the same underlying storage and channel.
#[derive(Clone, Debug)]
pub struct EventBus {
    log: Arc<Mutex<Vec<AgentEvent>>>,
    sender: broadcast::Sender<AgentEvent>,
}

impl EventBus {
    /// Create a new empty event bus.
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(CHANNEL_CAPACITY);
        Self {
            log: Arc::new(Mutex::new(Vec::new())),
            sender,
        }
    }

    /// Append an event to the durable log and broadcast it to all live subscribers.
    ///
    /// This is the **only** way events enter the bus; the log is append-only.
    pub fn emit(&self, event: AgentEvent) {
        {
            let mut log = self.log.lock().unwrap();
            log.push(event.clone());
        }
        // Broadcast returns an error only when there are no receivers.
        // That is not an error condition — events still land in the durable log.
        let _ = self.sender.send(event);
    }

    /// Subscribe to live events. The returned receiver will receive all events
    /// emitted after the call to `subscribe`.
    ///
    /// To replay past events, use [`EventBus::all_events`].
    pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent> {
        self.sender.subscribe()
    }

    /// Return a snapshot of all events in the durable log, in emission order.
    pub fn all_events(&self) -> Vec<AgentEvent> {
        self.log.lock().unwrap().clone()
    }

    /// Return all events associated with a specific session.
    pub fn events_for_session(&self, session_id: &SessionId) -> Vec<AgentEvent> {
        self.all_events()
            .into_iter()
            .filter(|e| event_session_id(e).as_ref() == Some(session_id))
            .collect()
    }

    /// Return all events associated with a specific run.
    pub fn events_for_run(&self, run_id: &RunId) -> Vec<AgentEvent> {
        self.all_events()
            .into_iter()
            .filter(|e| event_run_id(e).as_ref() == Some(run_id))
            .collect()
    }

    /// Total number of events in the durable log.
    pub fn len(&self) -> usize {
        self.log.lock().unwrap().len()
    }

    /// Returns `true` if no events have been emitted.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract the `SessionId` from an event, if it carries one.
fn event_session_id(event: &AgentEvent) -> Option<SessionId> {
    match event {
        AgentEvent::SessionCreated { session_id, .. }
        | AgentEvent::SessionResumed { session_id, .. }
        | AgentEvent::SessionEnded { session_id, .. } => Some(session_id.clone()),
        AgentEvent::RunCreated { session_id, .. }
        | AgentEvent::RunStarted { session_id, .. } => Some(session_id.clone()),
        _ => None,
    }
}

/// Extract the `RunId` from an event, if it carries one.
fn event_run_id(event: &AgentEvent) -> Option<RunId> {
    match event {
        AgentEvent::RunCreated { run_id, .. }
        | AgentEvent::RunQueued { run_id, .. }
        | AgentEvent::RunStarted { run_id, .. }
        | AgentEvent::RunCompleted { run_id, .. }
        | AgentEvent::RunCancelled { run_id, .. }
        | AgentEvent::RunFailed { run_id, .. }
        | AgentEvent::InterruptRequested { run_id, .. }
        | AgentEvent::CancellationRequested { run_id, .. }
        | AgentEvent::PromptAssembled { run_id, .. }
        | AgentEvent::TokenChunk { run_id, .. }
        | AgentEvent::ToolCallRequested { run_id, .. }
        | AgentEvent::ToolResultSubmitted { run_id, .. }
        | AgentEvent::ToolExecutionStarted { run_id, .. }
        | AgentEvent::ToolStdout { run_id, .. }
        | AgentEvent::ToolStderr { run_id, .. }
        | AgentEvent::ToolExecutionCompleted { run_id, .. }
        | AgentEvent::ToolExecutionFailed { run_id, .. }
        | AgentEvent::ToolExecutionCancelled { run_id, .. }
        | AgentEvent::ContextBuilt { run_id, .. }
        | AgentEvent::ContextCompacted { run_id, .. } => Some(run_id.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SessionId;

    #[test]
    fn emit_appends_to_log() {
        let bus = EventBus::new();
        assert!(bus.is_empty());
        let sid = SessionId::new();
        bus.emit(AgentEvent::SessionCreated {
            session_id: sid.clone(),
            timestamp: chrono::Utc::now(),
        });
        assert_eq!(bus.len(), 1);
        bus.emit(AgentEvent::SessionEnded {
            session_id: sid.clone(),
            timestamp: chrono::Utc::now(),
        });
        assert_eq!(bus.len(), 2);
    }

    #[test]
    fn all_events_preserves_order() {
        let bus = EventBus::new();
        let sid = SessionId::new();
        let run_id = RunId::new();
        bus.emit(AgentEvent::SessionCreated {
            session_id: sid.clone(),
            timestamp: chrono::Utc::now(),
        });
        bus.emit(AgentEvent::RunCreated {
            run_id: run_id.clone(),
            session_id: sid.clone(),
            timestamp: chrono::Utc::now(),
        });
        let events = bus.all_events();
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], AgentEvent::SessionCreated { .. }));
        assert!(matches!(events[1], AgentEvent::RunCreated { .. }));
    }

    #[test]
    fn events_for_session_filters_correctly() {
        let bus = EventBus::new();
        let sid1 = SessionId::new();
        let sid2 = SessionId::new();
        bus.emit(AgentEvent::SessionCreated {
            session_id: sid1.clone(),
            timestamp: chrono::Utc::now(),
        });
        bus.emit(AgentEvent::SessionCreated {
            session_id: sid2.clone(),
            timestamp: chrono::Utc::now(),
        });
        let s1_events = bus.events_for_session(&sid1);
        assert_eq!(s1_events.len(), 1);
        assert!(matches!(s1_events[0], AgentEvent::SessionCreated { session_id: ref id, .. } if id == &sid1));
    }

    #[test]
    fn events_for_run_filters_correctly() {
        let bus = EventBus::new();
        let sid = SessionId::new();
        let run1 = RunId::new();
        let run2 = RunId::new();
        bus.emit(AgentEvent::RunCreated {
            run_id: run1.clone(),
            session_id: sid.clone(),
            timestamp: chrono::Utc::now(),
        });
        bus.emit(AgentEvent::RunCreated {
            run_id: run2.clone(),
            session_id: sid.clone(),
            timestamp: chrono::Utc::now(),
        });
        bus.emit(AgentEvent::RunCompleted {
            run_id: run1.clone(),
            timestamp: chrono::Utc::now(),
        });
        let run1_events = bus.events_for_run(&run1);
        assert_eq!(run1_events.len(), 2);
        let run2_events = bus.events_for_run(&run2);
        assert_eq!(run2_events.len(), 1);
    }

    #[tokio::test]
    async fn subscriber_receives_live_events() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();
        let sid = SessionId::new();
        bus.emit(AgentEvent::SessionCreated {
            session_id: sid.clone(),
            timestamp: chrono::Utc::now(),
        });
        let received = rx.try_recv().unwrap();
        assert!(matches!(received, AgentEvent::SessionCreated { .. }));
    }

    #[test]
    fn clone_shares_log() {
        let bus1 = EventBus::new();
        let bus2 = bus1.clone();
        let sid = SessionId::new();
        bus1.emit(AgentEvent::SessionCreated {
            session_id: sid,
            timestamp: chrono::Utc::now(),
        });
        // Both clones see the same log.
        assert_eq!(bus2.len(), 1);
    }
}
