//! Event store trait and JSONL codec.

use crate::{record::EventRecord, AgentEvent, EventLogError};
use agent_core::types::{RunId, SessionId};

/// An append-only store for [`AgentEvent`]s.
#[async_trait::async_trait]
pub trait EventStore: Send + Sync {
    /// Append a single event to the log.
    async fn append(&self, event: &AgentEvent) -> Result<(), EventLogError>;
    /// Stream all events for a session in order.
    async fn replay_session(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<AgentEvent>, EventLogError>;
    /// Stream all events for a specific run in order.
    async fn replay_run(&self, run_id: &RunId) -> Result<Vec<AgentEvent>, EventLogError>;
}

/// Serialize an event to a JSONL line (no trailing newline included).
pub fn encode_event(event: &AgentEvent) -> Result<String, EventLogError> {
    Ok(serde_json::to_string(event)?)
}

/// Deserialize a JSONL line to an event.
pub fn decode_event(line: &str) -> Result<AgentEvent, EventLogError> {
    Ok(serde_json::from_str(line)?)
}

/// Serialize an `EventRecord` to a JSONL line (no trailing newline).
pub fn encode_record(record: &EventRecord) -> Result<String, EventLogError> {
    Ok(serde_json::to_string(record)?)
}

/// Deserialize a JSONL line to an `EventRecord`.
pub fn decode_record(line: &str) -> Result<EventRecord, EventLogError> {
    Ok(serde_json::from_str(line)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::types::{RunId, SessionId};
    use chrono::Utc;

    #[test]
    fn roundtrip_session_created() {
        let event = AgentEvent::SessionCreated {
            session_id: SessionId::new(),
            timestamp: Utc::now(),
        };
        let line = encode_event(&event).unwrap();
        let decoded = decode_event(&line).unwrap();
        // Verify tag is present
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["type"], "session_created");
        // Verify roundtrip preserves structure
        let re_encoded = encode_event(&decoded).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&line).unwrap(),
            serde_json::from_str::<serde_json::Value>(&re_encoded).unwrap()
        );
    }

    #[test]
    fn roundtrip_run_started() {
        use agent_core::types::{ModelId, ProviderId};
        let event = AgentEvent::RunStarted {
            run_id: RunId::new(),
            session_id: SessionId::new(),
            provider: ProviderId::new("openai"),
            model: ModelId::new("gpt-4o"),
            timestamp: Utc::now(),
        };
        let line = encode_event(&event).unwrap();
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["type"], "run_started");
    }
}
