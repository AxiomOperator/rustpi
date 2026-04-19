//! Replay failure-mode tests for the event-log crate.
//!
//! These tests exercise `FileEventStore::replay_all_tolerant` against corrupt
//! and truncated JSONL log files to verify the tolerant reader:
//!   1. skips a truncated final line
//!   2. skips a corrupt line in the middle and returns the surrounding valid events

use std::path::{Path, PathBuf};

use event_log::{AgentEvent, EventRecord, FileEventStore};
use agent_core::types::SessionId;
use chrono::Utc;

// ── helpers ──────────────────────────────────────────────────────────────────

fn test_artifacts_path(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-artifacts");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn cleanup(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// Serialize one `EventRecord` to a JSONL line (no trailing newline).
fn encode(record: &EventRecord) -> String {
    serde_json::to_string(record).unwrap()
}

fn make_session_event(seq: u64) -> EventRecord {
    EventRecord::new(
        seq,
        AgentEvent::SessionCreated {
            session_id: SessionId::new(),
            timestamp: Utc::now(),
        },
    )
}

// ── tests ─────────────────────────────────────────────────────────────────────

/// A JSONL log whose last line is truncated (e.g. process crashed mid-write).
/// `replay_all_tolerant` must skip the incomplete line and return only the
/// valid records that preceded it.
#[tokio::test]
async fn partial_jsonl_line_at_eof_is_skipped() {
    let path = test_artifacts_path("replay_partial_eof.log");
    cleanup(&path);

    // Write one complete JSONL record followed by a truncated line.
    let good = make_session_event(0);
    let truncated = r#"{"event_type":"run_started","run_id"#; // cut off mid-JSON
    let content = format!("{}\n{}", encode(&good), truncated);
    std::fs::write(&path, &content).unwrap();

    let store = FileEventStore::open(&path).await.unwrap();
    let records = store.replay_all_tolerant().await;

    assert_eq!(
        records.len(),
        1,
        "tolerant replay must skip the truncated line and return only the 1 valid record"
    );
    assert_eq!(records[0].seq, 0);

    cleanup(&path);
}

/// A JSONL log with valid → corrupt → valid lines.
/// `replay_all_tolerant` must skip the corrupt line and return the 2 valid records.
#[tokio::test]
async fn corrupted_event_json_in_middle_is_skipped() {
    let path = test_artifacts_path("replay_corrupt_middle.log");
    cleanup(&path);

    let first = make_session_event(0);
    let corrupt = r#"{"seq":1,"appended_at":"not-a-date","event":{{broken}}"#;
    let third = make_session_event(2);

    let content = format!("{}\n{}\n{}", encode(&first), corrupt, encode(&third));
    std::fs::write(&path, &content).unwrap();

    let store = FileEventStore::open(&path).await.unwrap();
    let records = store.replay_all_tolerant().await;

    assert_eq!(
        records.len(),
        2,
        "tolerant replay must skip the corrupt middle line and return 2 valid records"
    );
    assert_eq!(records[0].seq, 0, "first record seq mismatch");
    assert_eq!(records[1].seq, 2, "third record seq mismatch");

    cleanup(&path);
}
