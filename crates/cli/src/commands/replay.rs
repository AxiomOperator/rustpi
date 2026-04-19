//! `rustpi replay` — display the event timeline for a session.
//!
//! # Production path
//! When `~/.rustpi/events.jsonl` exists it is loaded via `FileEventStore` and
//! displayed with full fidelity, including tolerant parsing of any corrupt lines.
//!
//! # Demo / development path
//! When no persistent log is found, a small in-memory store is populated with
//! synthetic demo events so the command always produces meaningful output.

use agent_core::types::{ModelId, ProviderId, RunId, SessionId};
use chrono::Utc;
use event_log::{AgentEvent, FileEventStore, MemoryEventStore, ReplayReader};

use crate::{
    args::OutputFormat,
    error::{CliError, CliResult},
    output::Output,
};

/// Default event log path relative to the user's home directory.
const DEFAULT_LOG_RELATIVE: &str = ".rustpi/events.jsonl";

pub async fn replay_command(
    session_id: Option<&str>,
    audit_only: bool,
    failures_only: bool,
    output: &Output,
) -> CliResult<()> {
    let reader = load_reader().await;

    let mut entries = reader.format_timeline();

    // Apply optional filters.
    if let Some(sid) = session_id {
        entries.retain(|e| e.session_id.as_deref() == Some(sid));
    }
    if audit_only {
        entries.retain(|e| e.is_audit);
    }
    if failures_only {
        entries.retain(|e| e.is_failure);
    }

    if output.format == OutputFormat::Json {
        let data = serde_json::to_value(&entries)
            .map_err(|e| CliError::Other(e.to_string()))?;
        output.print_success("timeline", &data);
    } else {
        if entries.is_empty() {
            println!("(no events found)");
        } else {
            for entry in &entries {
                println!(
                    "[seq={:>4}] {}  {:<35}  {}",
                    entry.seq,
                    entry.timestamp.format("%H:%M:%S%.3f"),
                    entry.event_type,
                    entry.summary,
                );
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Loader
// ---------------------------------------------------------------------------

async fn load_reader() -> ReplayReader {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let path = std::path::PathBuf::from(&home).join(DEFAULT_LOG_RELATIVE);

    if path.exists() {
        if let Ok(store) = FileEventStore::open(&path).await {
            return ReplayReader::from_file_tolerant(&store).await;
        }
    }

    // No persistent log found — build a demo store so the command always
    // produces output rather than silently printing nothing.
    let store = MemoryEventStore::new();
    build_demo_events(&store).await;
    ReplayReader::from_memory(&store)
}

async fn build_demo_events(store: &MemoryEventStore) {
    use event_log::store::EventStore;

    let sid = SessionId::new();
    let run_id = RunId::new();

    let _ = store
        .append(&AgentEvent::SessionCreated {
            session_id: sid.clone(),
            timestamp: Utc::now(),
        })
        .await;
    let _ = store
        .append(&AgentEvent::RunStarted {
            run_id: run_id.clone(),
            session_id: sid.clone(),
            provider: ProviderId::new("demo"),
            model: ModelId::new("demo-model"),
            timestamp: Utc::now(),
        })
        .await;
    let _ = store
        .append(&AgentEvent::PromptAssembled {
            run_id: run_id.clone(),
            section_count: 1,
            estimated_tokens: 128,
            timestamp: Utc::now(),
        })
        .await;
    let _ = store
        .append(&AgentEvent::RunCompleted {
            run_id: run_id.clone(),
            timestamp: Utc::now(),
        })
        .await;
    let _ = store
        .append(&AgentEvent::SessionEnded {
            session_id: sid.clone(),
            timestamp: Utc::now(),
        })
        .await;
}
