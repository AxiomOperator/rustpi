//! Append-only event log for the agent runtime.
//!
//! Events are serialized as JSONL (one JSON object per line) for streaming-safe
//! storage and replay. The log is the source of truth for runtime state.
//!
//! # Serialization format
//! Each line is a complete, self-contained JSON object representing one [`AgentEvent`].
//! Lines must not be modified after being written (append-only guarantee).

pub mod error;
pub mod file_store;
pub mod memory_store;
pub mod record;
pub mod replay;
pub mod store;

pub use agent_core::types::AgentEvent;
pub use error::EventLogError;
pub use file_store::FileEventStore;
pub use memory_store::MemoryEventStore;
pub use record::{AuditKind, AuditRecord, EventRecord};
pub use replay::ReplayReader;
pub use store::{decode_event, decode_record, encode_event, encode_record, EventStore};
