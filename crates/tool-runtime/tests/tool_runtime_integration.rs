//! Integration tests for the tool runtime, validating behaviour through the ToolRunner.
//!
//! API discoveries (noted inline below):
//! - `ToolRunner::new(registry, default_timeout)` — approval defaults to AutoApprove.
//! - Runner-level timeout (via `ToolConfig::timeout`) early-returns with `?`, so it does NOT
//!   emit a `ToolFailed` event; only `ToolStarted` is emitted in that path.
//! - The shell tool's own `timeout_secs` arg causes it to return `Err(ToolError::Timeout)`,
//!   which *does* flow through the runner's `Err` match arm and emits `ToolFailed`.
//! - `ToolStdout` events require both `run_id` and `call_id` to be `Some` inside
//!   `SubprocessConfig`; ShellTool passes `run_id: None`, so ToolStdout events are not emitted
//!   via the runner path — stdout is captured only in `ToolResult::output["stdout"]`.
//! - Approval is only checked when `sensitivity >= High` OR `require_approval == true`;
//!   Safe-sensitivity tools bypass the hook entirely.

use std::sync::Arc;
use std::time::Duration;

use agent_core::types::{AgentEvent, RunId, ToolCall};
use serde_json::json;
use tempfile::TempDir;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use tool_runtime::{
    approval::{AllowList, AutoApprove, DenyAbove},
    overwrite_policy::OverwritePolicy,
    path_safety::PathSafetyPolicy,
    registry::ToolRegistry,
    runner::ToolRunner,
    schema::{ToolConfig, ToolSensitivity},
    tools::{
        edit::EditTool,
        file::{ReadFileTool, WriteFileTool},
        search::SearchTool,
        shell::ShellTool,
    },
    ToolError,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_policy(dir: &TempDir) -> Arc<PathSafetyPolicy> {
    Arc::new(PathSafetyPolicy::new([dir.path()]))
}

/// Build a registry containing all five built-in tools.
/// If `event_tx` is provided it is forwarded to `ShellTool` for stdout streaming.
fn make_registry(
    policy: Arc<PathSafetyPolicy>,
    event_tx: Option<broadcast::Sender<AgentEvent>>,
) -> Arc<ToolRegistry> {
    let mut reg = ToolRegistry::default();
    let shell = match event_tx {
        Some(tx) => ShellTool::new().with_event_tx(tx),
        None => ShellTool::new(),
    };
    reg.register(Arc::new(shell));
    reg.register(Arc::new(ReadFileTool::new(policy.clone())));
    reg.register(Arc::new(WriteFileTool::new(policy.clone())));
    reg.register(Arc::new(SearchTool::new(policy.clone())));
    reg.register(Arc::new(EditTool::new(policy.clone())));
    Arc::new(reg)
}

fn runner_in(dir: &TempDir) -> ToolRunner {
    let reg = make_registry(make_policy(dir), None);
    ToolRunner::new(reg, Duration::from_secs(30))
}

fn runner_with_events(dir: &TempDir, tx: broadcast::Sender<AgentEvent>) -> ToolRunner {
    let reg = make_registry(make_policy(dir), Some(tx.clone()));
    ToolRunner::new(reg, Duration::from_secs(30)).with_event_tx(tx)
}

fn shell_call(id: &str, cmd: &str) -> ToolCall {
    ToolCall {
        id: id.to_string(),
        name: "shell".to_string(),
        arguments: json!({ "command": cmd }),
    }
}

fn fresh_run_id() -> RunId {
    RunId::new()
}

/// Drain all currently-buffered events from a broadcast receiver.
fn drain_events(rx: &mut broadcast::Receiver<AgentEvent>) -> Vec<AgentEvent> {
    let mut out = Vec::new();
    while let Ok(e) = rx.try_recv() {
        out.push(e);
    }
    out
}

// ── A. Timeout enforcement (through runner) ───────────────────────────────────

/// Runner-level timeout: the outer `tokio::time::timeout` fires; function returns early via `?`.
#[tokio::test]
async fn runner_timeout_returns_error() {
    let dir = TempDir::new().unwrap();
    let runner = runner_in(&dir);

    let err = runner
        .execute(
            shell_call("a1", "sleep 60"),
            ToolConfig {
                timeout: Some(Duration::from_millis(300)),
                run_id: Some(fresh_run_id()),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();

    assert!(
        matches!(err, ToolError::Timeout(_)),
        "expected Timeout, got: {err:?}"
    );
}

/// Runner-level timeout emits ToolStarted; the early-return path does NOT emit ToolFailed.
#[tokio::test]
async fn runner_timeout_emits_tool_started_but_not_failed() {
    let dir = TempDir::new().unwrap();
    let (tx, mut rx) = broadcast::channel(32);
    let runner = runner_with_events(&dir, tx);

    let _ = runner
        .execute(
            shell_call("a2", "sleep 60"),
            ToolConfig {
                timeout: Some(Duration::from_millis(300)),
                run_id: Some(fresh_run_id()),
                ..Default::default()
            },
        )
        .await;

    let events = drain_events(&mut rx);
    assert!(
        events.iter().any(|e| matches!(e, AgentEvent::ToolStarted { .. })),
        "ToolStarted must be emitted before timeout; events: {events:?}"
    );
    // The runner early-returns via `?` on Elapsed, bypassing the Err match arm.
    assert!(
        !events.iter().any(|e| matches!(e, AgentEvent::ToolFailed { .. })),
        "ToolFailed is NOT emitted on runner-level timeout (early-return path)"
    );
}

/// When the shell tool's own `timeout_secs` fires it returns `Err(ToolError::Timeout)`.
/// That error flows through the runner's match arm, which DOES emit ToolFailed.
#[tokio::test]
async fn shell_internal_timeout_emits_tool_failed_event() {
    let dir = TempDir::new().unwrap();
    let (tx, mut rx) = broadcast::channel(32);
    let runner = runner_with_events(&dir, tx);

    let result = runner
        .execute_simple(
            ToolCall {
                id: "a3".into(),
                name: "shell".into(),
                // timeout_secs: 0 makes the shell tool's own subprocess timeout fire immediately
                arguments: json!({ "command": "sleep 10", "timeout_secs": 0 }),
            },
            fresh_run_id(),
        )
        .await;

    assert!(result.is_err());
    let events = drain_events(&mut rx);
    assert!(
        events.iter().any(|e| matches!(e, AgentEvent::ToolStarted { .. })),
        "ToolStarted must be emitted"
    );
    assert!(
        events.iter().any(|e| matches!(e, AgentEvent::ToolFailed { .. })),
        "ToolFailed must be emitted when shell's own timeout fires"
    );
}

// ── B. Cancellation enforcement (through runner) ──────────────────────────────

#[tokio::test]
async fn cancellation_stops_tool_and_returns_error() {
    let dir = TempDir::new().unwrap();
    let runner = runner_in(&dir);

    let token = CancellationToken::new();
    let token_clone = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        token_clone.cancel();
    });

    let err = runner
        .execute(
            shell_call("b1", "sleep 60"),
            ToolConfig {
                cancel: Some(token),
                run_id: Some(fresh_run_id()),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();

    assert!(
        matches!(err, ToolError::Cancelled),
        "expected Cancelled, got: {err:?}"
    );
}

#[tokio::test]
async fn cancellation_emits_tool_cancelled_event() {
    let dir = TempDir::new().unwrap();
    let (tx, mut rx) = broadcast::channel(32);
    let runner = runner_with_events(&dir, tx);

    let token = CancellationToken::new();
    let token_clone = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        token_clone.cancel();
    });

    let _ = runner
        .execute(
            shell_call("b2", "sleep 60"),
            ToolConfig {
                cancel: Some(token),
                run_id: Some(fresh_run_id()),
                ..Default::default()
            },
        )
        .await;

    let events = drain_events(&mut rx);
    assert!(
        events.iter().any(|e| matches!(e, AgentEvent::ToolCancelled { .. })),
        "ToolCancelled must be emitted; events: {events:?}"
    );
    // ToolFailed should NOT appear — cancellation uses a distinct event.
    assert!(
        !events.iter().any(|e| matches!(e, AgentEvent::ToolFailed { .. })),
        "ToolFailed must NOT appear on cancellation"
    );
}

// ── C. Non-zero exit handling ─────────────────────────────────────────────────

#[tokio::test]
async fn nonzero_exit_is_ok_result_with_captured_code() {
    let dir = TempDir::new().unwrap();
    let runner = runner_in(&dir);

    let result = runner
        .execute_simple(shell_call("c1", "exit 1"), fresh_run_id())
        .await
        .expect("non-zero exit should NOT be a runtime error");

    assert!(!result.success, "success flag must be false");
    assert_eq!(result.output["exit_code"], 1, "exit_code must be 1");
}

#[tokio::test]
async fn nonzero_exit_emits_tool_completed_with_exit_code() {
    let dir = TempDir::new().unwrap();
    let (tx, mut rx) = broadcast::channel(32);
    let runner = runner_with_events(&dir, tx);

    let _ = runner
        .execute_simple(shell_call("c2", "exit 2"), fresh_run_id())
        .await;

    let events = drain_events(&mut rx);
    let completed = events.iter().find_map(|e| {
        if let AgentEvent::ToolCompleted { exit_code, .. } = e {
            Some(*exit_code)
        } else {
            None
        }
    });
    assert_eq!(
        completed,
        Some(Some(2)),
        "ToolCompleted with exit_code=2 must be emitted; events: {events:?}"
    );
}

// ── D. Approval hook behaviour ────────────────────────────────────────────────

#[tokio::test]
async fn deny_above_high_blocks_critical_shell_tool() {
    let dir = TempDir::new().unwrap();
    let reg = make_registry(make_policy(&dir), None);
    let runner = ToolRunner::new(reg, Duration::from_secs(30))
        .with_approval(Arc::new(DenyAbove { threshold: ToolSensitivity::High }));

    let err = runner
        .execute_simple(shell_call("d1", "echo hello"), fresh_run_id())
        .await
        .unwrap_err();

    assert!(
        matches!(err, ToolError::PolicyDenied(_)),
        "expected PolicyDenied, got: {err:?}"
    );
}

#[tokio::test]
async fn auto_approve_allows_critical_shell_tool() {
    let dir = TempDir::new().unwrap();
    let reg = make_registry(make_policy(&dir), None);
    let runner = ToolRunner::new(reg, Duration::from_secs(30)).with_approval(Arc::new(AutoApprove));

    let result = runner
        .execute_simple(shell_call("d2", "echo hello"), fresh_run_id())
        .await;

    assert!(result.is_ok(), "AutoApprove should allow shell; err: {result:?}");
}

#[tokio::test]
async fn allow_list_denies_tool_not_in_list() {
    let dir = TempDir::new().unwrap();
    let reg = make_registry(make_policy(&dir), None);
    // shell (Critical) is not in the list → approval check → Denied
    let runner = ToolRunner::new(reg, Duration::from_secs(30))
        .with_approval(Arc::new(AllowList::new(["read_file"])));

    let err = runner
        .execute_simple(shell_call("d3", "echo hello"), fresh_run_id())
        .await
        .unwrap_err();

    assert!(
        matches!(err, ToolError::PolicyDenied(_)),
        "expected PolicyDenied for unlisted tool; got: {err:?}"
    );
}

#[tokio::test]
async fn allow_list_allows_listed_high_sensitivity_tool() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("allowed.txt");
    let reg = make_registry(make_policy(&dir), None);
    // write_file (High) is in the list → Approved
    let runner = ToolRunner::new(reg, Duration::from_secs(30))
        .with_approval(Arc::new(AllowList::new(["write_file"])));

    let result = runner
        .execute_simple(
            ToolCall {
                id: "d4".into(),
                name: "write_file".into(),
                arguments: json!({
                    "path": file_path.to_string_lossy(),
                    "content": "allowed"
                }),
            },
            fresh_run_id(),
        )
        .await;

    assert!(result.is_ok(), "AllowList should allow write_file; err: {result:?}");
}

/// Approval is NOT invoked for Safe-sensitivity tools regardless of the hook.
#[tokio::test]
async fn safe_sensitivity_tool_bypasses_approval_hook() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("bypass.txt");
    std::fs::write(&file_path, "safe").unwrap();

    let reg = make_registry(make_policy(&dir), None);
    // AllowList that lists nothing — but read_file (Safe) should bypass it entirely.
    let runner = ToolRunner::new(reg, Duration::from_secs(30))
        .with_approval(Arc::new(AllowList::new([] as [&str; 0])));

    let result = runner
        .execute_simple(
            ToolCall {
                id: "d5".into(),
                name: "read_file".into(),
                arguments: json!({ "path": file_path.to_string_lossy() }),
            },
            fresh_run_id(),
        )
        .await;

    assert!(result.is_ok(), "Safe tool should bypass approval hook; err: {result:?}");
}

// ── E. Event emission ─────────────────────────────────────────────────────────

#[tokio::test]
async fn successful_tool_emits_started_and_completed() {
    let dir = TempDir::new().unwrap();
    let (tx, mut rx) = broadcast::channel(32);
    let runner = runner_with_events(&dir, tx);

    runner
        .execute_simple(shell_call("e1", "echo events"), fresh_run_id())
        .await
        .unwrap();

    let events = drain_events(&mut rx);
    assert!(
        events.iter().any(|e| matches!(e, AgentEvent::ToolStarted { .. })),
        "ToolStarted must be emitted; events: {events:?}"
    );
    assert!(
        events.iter().any(|e| matches!(e, AgentEvent::ToolCompleted { .. })),
        "ToolCompleted must be emitted; events: {events:?}"
    );
}

#[tokio::test]
async fn tool_started_event_carries_correct_tool_name() {
    let dir = TempDir::new().unwrap();
    let (tx, mut rx) = broadcast::channel(32);
    let runner = runner_with_events(&dir, tx);

    runner
        .execute_simple(shell_call("e2", "true"), fresh_run_id())
        .await
        .unwrap();

    let events = drain_events(&mut rx);
    let started_name = events.iter().find_map(|e| {
        if let AgentEvent::ToolStarted { tool_name, .. } = e {
            Some(tool_name.clone())
        } else {
            None
        }
    });
    assert_eq!(started_name.as_deref(), Some("shell"));
}

#[tokio::test]
async fn approval_denied_emits_tool_failed_not_started() {
    let dir = TempDir::new().unwrap();
    let (tx, mut rx) = broadcast::channel(32);
    let reg = make_registry(make_policy(&dir), None);
    let runner = ToolRunner::new(reg, Duration::from_secs(30))
        .with_approval(Arc::new(DenyAbove { threshold: ToolSensitivity::High }))
        .with_event_tx(tx);

    let _ = runner
        .execute_simple(shell_call("e3", "echo hello"), fresh_run_id())
        .await;

    let events = drain_events(&mut rx);
    assert!(
        events.iter().any(|e| matches!(e, AgentEvent::ToolFailed { .. })),
        "ToolFailed must be emitted on denial; events: {events:?}"
    );
    // ToolStarted is NOT emitted because approval check happens before it
    assert!(
        !events.iter().any(|e| matches!(e, AgentEvent::ToolStarted { .. })),
        "ToolStarted must NOT be emitted when approval is denied"
    );
}

// ── F. Path safety (through file tools and runner) ────────────────────────────

#[tokio::test]
async fn read_file_blocked_outside_allowed_root() {
    let dir = TempDir::new().unwrap();
    let runner = runner_in(&dir);

    let err = runner
        .execute_simple(
            ToolCall {
                id: "f1".into(),
                name: "read_file".into(),
                arguments: json!({ "path": "/etc/passwd" }),
            },
            fresh_run_id(),
        )
        .await
        .unwrap_err();

    assert!(
        matches!(err, ToolError::PathTraversal(_)),
        "expected PathTraversal; got: {err:?}"
    );
}

#[tokio::test]
async fn write_file_blocked_outside_allowed_root() {
    let dir = TempDir::new().unwrap();
    let runner = runner_in(&dir);

    let err = runner
        .execute_simple(
            ToolCall {
                id: "f2".into(),
                name: "write_file".into(),
                arguments: json!({ "path": "/etc/evil.txt", "content": "bad" }),
            },
            fresh_run_id(),
        )
        .await
        .unwrap_err();

    assert!(
        matches!(err, ToolError::PathTraversal(_)),
        "expected PathTraversal; got: {err:?}"
    );
}

#[tokio::test]
async fn edit_file_blocked_outside_allowed_root() {
    let dir = TempDir::new().unwrap();
    let runner = runner_in(&dir);

    let err = runner
        .execute_simple(
            ToolCall {
                id: "f3".into(),
                name: "edit_file".into(),
                arguments: json!({
                    "path": "/etc/hosts",
                    "old_str": "localhost",
                    "new_str": "evil"
                }),
            },
            fresh_run_id(),
        )
        .await
        .unwrap_err();

    assert!(
        matches!(err, ToolError::PathTraversal(_)),
        "expected PathTraversal; got: {err:?}"
    );
}

#[tokio::test]
async fn path_traversal_via_dotdot_is_blocked() {
    let dir = TempDir::new().unwrap();
    let runner = runner_in(&dir);

    // Build a traversal path: <root>/../etc/passwd
    let traversal = format!("{}/../etc/passwd", dir.path().display());
    let err = runner
        .execute_simple(
            ToolCall {
                id: "f4".into(),
                name: "read_file".into(),
                arguments: json!({ "path": traversal }),
            },
            fresh_run_id(),
        )
        .await
        .unwrap_err();

    assert!(
        matches!(err, ToolError::PathTraversal(_)),
        "dotdot traversal must be blocked; got: {err:?}"
    );
}

// ── G. File read/write round-trip through runner ──────────────────────────────

#[tokio::test]
async fn write_then_read_file_round_trip() {
    let dir = TempDir::new().unwrap();
    let runner = runner_in(&dir);
    let file_path = dir.path().join("roundtrip.txt");
    let content = "integration test round-trip content";

    let write_result = runner
        .execute_simple(
            ToolCall {
                id: "g1".into(),
                name: "write_file".into(),
                arguments: json!({
                    "path": file_path.to_string_lossy(),
                    "content": content
                }),
            },
            fresh_run_id(),
        )
        .await
        .expect("write should succeed");
    assert!(write_result.success);

    let read_result = runner
        .execute_simple(
            ToolCall {
                id: "g2".into(),
                name: "read_file".into(),
                arguments: json!({ "path": file_path.to_string_lossy() }),
            },
            fresh_run_id(),
        )
        .await
        .expect("read should succeed");
    assert!(read_result.success);
    assert_eq!(
        read_result.output["content"].as_str().unwrap(),
        content,
        "read content must match written content"
    );
}

#[tokio::test]
async fn write_file_creates_nested_dirs_when_flagged() {
    let dir = TempDir::new().unwrap();
    let runner = runner_in(&dir);
    let file_path = dir.path().join("a/b/c/deep.txt");

    let result = runner
        .execute_simple(
            ToolCall {
                id: "g3".into(),
                name: "write_file".into(),
                arguments: json!({
                    "path": file_path.to_string_lossy(),
                    "content": "deep",
                    "create_dirs": true
                }),
            },
            fresh_run_id(),
        )
        .await
        .expect("write with create_dirs should succeed");

    assert!(result.success);
    assert!(file_path.exists(), "nested file should exist on disk");
}

// ── H. Search tool correctness through runner ─────────────────────────────────

#[tokio::test]
async fn search_finds_matches_across_multiple_files() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("a.txt"), "foo bar baz\nqux quux\n").unwrap();
    std::fs::write(dir.path().join("b.txt"), "hello foo world\n").unwrap();
    std::fs::write(dir.path().join("c.txt"), "no match here\n").unwrap();

    let runner = runner_in(&dir);

    let result = runner
        .execute_simple(
            ToolCall {
                id: "h1".into(),
                name: "search".into(),
                arguments: json!({
                    "path": dir.path().to_string_lossy(),
                    "pattern": "foo"
                }),
            },
            fresh_run_id(),
        )
        .await
        .expect("search should succeed");

    assert!(result.success);
    assert_eq!(
        result.output["match_count"], 2,
        "expected 2 matches for 'foo'; output: {:?}", result.output
    );
}

#[tokio::test]
async fn search_returns_empty_when_no_match() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("file.txt"), "nothing relevant\n").unwrap();

    let runner = runner_in(&dir);

    let result = runner
        .execute_simple(
            ToolCall {
                id: "h2".into(),
                name: "search".into(),
                arguments: json!({
                    "path": dir.path().to_string_lossy(),
                    "pattern": "xyz_not_found_99999"
                }),
            },
            fresh_run_id(),
        )
        .await
        .expect("search with no matches should still succeed");

    assert!(result.success);
    assert_eq!(result.output["match_count"], 0);
}

#[tokio::test]
async fn search_respects_max_results_limit() {
    let dir = TempDir::new().unwrap();
    // Write 10 matching lines
    let content = (1..=10).map(|i| format!("match line {i}")).collect::<Vec<_>>().join("\n");
    std::fs::write(dir.path().join("big.txt"), content).unwrap();

    let runner = runner_in(&dir);

    let result = runner
        .execute_simple(
            ToolCall {
                id: "h3".into(),
                name: "search".into(),
                arguments: json!({
                    "path": dir.path().to_string_lossy(),
                    "pattern": "match",
                    "max_results": 3
                }),
            },
            fresh_run_id(),
        )
        .await
        .expect("search should succeed");

    assert!(result.success);
    assert_eq!(result.output["match_count"], 3, "should respect max_results");
    assert_eq!(result.output["truncated"], true, "truncated flag must be true");
}

#[tokio::test]
async fn search_blocked_outside_allowed_root() {
    let dir = TempDir::new().unwrap();
    let runner = runner_in(&dir);

    let err = runner
        .execute_simple(
            ToolCall {
                id: "h4".into(),
                name: "search".into(),
                arguments: json!({ "path": "/etc", "pattern": "root" }),
            },
            fresh_run_id(),
        )
        .await
        .unwrap_err();

    assert!(
        matches!(err, ToolError::PathTraversal(_)),
        "search outside root must be blocked; got: {err:?}"
    );
}

// ── I. Edit tool through runner ───────────────────────────────────────────────

#[tokio::test]
async fn edit_file_success_through_runner() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("edit_me.txt");
    std::fs::write(&file_path, "original content here\n").unwrap();

    let runner = runner_in(&dir);

    let result = runner
        .execute_simple(
            ToolCall {
                id: "i1".into(),
                name: "edit_file".into(),
                arguments: json!({
                    "path": file_path.to_string_lossy(),
                    "old_str": "original",
                    "new_str": "modified"
                }),
            },
            fresh_run_id(),
        )
        .await
        .expect("edit should succeed");

    assert!(result.success);
    assert_eq!(result.output["replacements"], 1);

    let disk = std::fs::read_to_string(&file_path).unwrap();
    assert!(disk.contains("modified"), "disk content should contain 'modified'");
    assert!(!disk.contains("original"), "disk content should not contain 'original'");
}

#[tokio::test]
async fn edit_file_nonexistent_old_str_surfaces_error() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("no_match.txt");
    std::fs::write(&file_path, "some content\n").unwrap();

    let runner = runner_in(&dir);

    let err = runner
        .execute_simple(
            ToolCall {
                id: "i2".into(),
                name: "edit_file".into(),
                arguments: json!({
                    "path": file_path.to_string_lossy(),
                    "old_str": "DOES_NOT_EXIST_IN_FILE",
                    "new_str": "replacement"
                }),
            },
            fresh_run_id(),
        )
        .await
        .unwrap_err();

    assert!(
        matches!(err, ToolError::InvalidArguments { .. }),
        "missing old_str must surface InvalidArguments; got: {err:?}"
    );
}

#[tokio::test]
async fn edit_idempotent_replace_all_through_runner() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("multi.txt");
    std::fs::write(&file_path, "foo bar foo baz foo\n").unwrap();

    let runner = runner_in(&dir);

    let result = runner
        .execute_simple(
            ToolCall {
                id: "i3".into(),
                name: "edit_file".into(),
                arguments: json!({
                    "path": file_path.to_string_lossy(),
                    "old_str": "foo",
                    "new_str": "qux",
                    "replace_all": true
                }),
            },
            fresh_run_id(),
        )
        .await
        .expect("replace_all edit should succeed");

    assert!(result.success);
    assert_eq!(result.output["replacements"], 3);

    let disk = std::fs::read_to_string(&file_path).unwrap();
    assert!(!disk.contains("foo"), "all 'foo' must be replaced");
    assert_eq!(disk.matches("qux").count(), 3);
}

// ── J. Subprocess stdout captured in result ───────────────────────────────────
//
// Note: ToolStdout events require both run_id and call_id in SubprocessConfig.
// ShellTool currently passes run_id: None, so ToolStdout events are NOT emitted
// via the runner path. Multi-line stdout is verified via ToolResult::output["stdout"].

#[tokio::test]
async fn multiline_stdout_fully_captured_in_result() {
    let dir = TempDir::new().unwrap();
    let runner = runner_in(&dir);

    let result = runner
        .execute_simple(
            shell_call("j1", "printf 'line1\\nline2\\nline3\\n'"),
            fresh_run_id(),
        )
        .await
        .expect("printf should succeed");

    assert!(result.success);
    let stdout = result.output["stdout"].as_str().unwrap();
    assert!(stdout.contains("line1"), "stdout must contain line1");
    assert!(stdout.contains("line2"), "stdout must contain line2");
    assert!(stdout.contains("line3"), "stdout must contain line3");
}

#[tokio::test]
async fn stdout_and_stderr_are_separately_captured() {
    let dir = TempDir::new().unwrap();
    let runner = runner_in(&dir);

    let result = runner
        .execute_simple(
            shell_call("j2", "echo out_line; echo err_line >&2"),
            fresh_run_id(),
        )
        .await
        .expect("command should succeed");

    assert!(result.success);
    assert!(
        result.output["stdout"].as_str().unwrap().contains("out_line"),
        "stdout must contain out_line"
    );
    assert!(
        result.output["stderr"].as_str().unwrap().contains("err_line"),
        "stderr must contain err_line"
    );
}

// ── Misc ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn unregistered_tool_returns_not_found() {
    let dir = TempDir::new().unwrap();
    let runner = runner_in(&dir);

    let err = runner
        .execute_simple(
            ToolCall {
                id: "m1".into(),
                name: "nonexistent_tool".into(),
                arguments: json!({}),
            },
            fresh_run_id(),
        )
        .await
        .unwrap_err();

    assert!(
        matches!(err, ToolError::NotFound(_)),
        "expected NotFound; got: {err:?}"
    );
}

#[tokio::test]
async fn shell_tool_success_flag_true_for_exit_zero() {
    let dir = TempDir::new().unwrap();
    let runner = runner_in(&dir);

    let result = runner
        .execute_simple(shell_call("m2", "true"), fresh_run_id())
        .await
        .expect("true should succeed");

    assert!(result.success);
    assert_eq!(result.output["exit_code"], 0);
}

/// When `require_approval` is true the hook IS called even for Safe tools.
/// DenyAbove(High) will still Approve a Safe tool (Safe < High), so the call succeeds.
#[tokio::test]
async fn require_approval_flag_forces_hook_but_safe_still_approved_by_deny_above() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("safe.txt");
    std::fs::write(&file_path, "safe content").unwrap();

    let reg = make_registry(make_policy(&dir), None);
    let runner = ToolRunner::new(reg, Duration::from_secs(30))
        .with_approval(Arc::new(DenyAbove { threshold: ToolSensitivity::High }));

    // DenyAbove.check: Safe < High → Approved. Call must succeed.
    let result = runner
        .execute(
            ToolCall {
                id: "m3".into(),
                name: "read_file".into(),
                arguments: json!({ "path": file_path.to_string_lossy() }),
            },
            ToolConfig {
                require_approval: true,
                run_id: Some(fresh_run_id()),
                ..Default::default()
            },
        )
        .await;

    assert!(
        result.is_ok(),
        "DenyAbove(High) approves Safe tools; got: {result:?}"
    );
}

#[tokio::test]
async fn require_approval_blocks_safe_tool_when_deny_all() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("safe2.txt");
    std::fs::write(&file_path, "content").unwrap();

    let reg = make_registry(make_policy(&dir), None);
    // AllowList with nothing — denies all tools including Safe ones when require_approval=true
    let runner = ToolRunner::new(reg, Duration::from_secs(30))
        .with_approval(Arc::new(AllowList::new([] as [&str; 0])));

    let err = runner
        .execute(
            ToolCall {
                id: "m4".into(),
                name: "read_file".into(),
                arguments: json!({ "path": file_path.to_string_lossy() }),
            },
            ToolConfig {
                require_approval: true,
                run_id: Some(fresh_run_id()),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();

    assert!(
        matches!(err, ToolError::PolicyDenied(_)),
        "require_approval must force hook call even for Safe tools; got: {err:?}"
    );
}

#[tokio::test]
async fn tool_runner_emits_approval_denied_event() {
    let dir = TempDir::new().unwrap();
    let (tx, mut rx) = broadcast::channel::<AgentEvent>(32);
    let reg = make_registry(make_policy(&dir), None);
    let run_id = fresh_run_id();

    // DenyAbove(High) will deny Critical-sensitivity tools (shell is critical).
    let runner = ToolRunner::new(reg, Duration::from_secs(30))
        .with_approval(Arc::new(DenyAbove { threshold: ToolSensitivity::High }))
        .with_event_tx(tx);

    let err = runner
        .execute(
            shell_call("deny-test", "echo hello"),
            ToolConfig {
                run_id: Some(run_id.clone()),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();

    assert!(
        matches!(err, ToolError::PolicyDenied(_)),
        "expected PolicyDenied, got: {err:?}"
    );

    // Collect all events from the channel.
    let mut events = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        events.push(ev);
    }

    let has_approval_denied = events.iter().any(|e| matches!(e, AgentEvent::ApprovalDenied { .. }));
    let has_tool_failed = events.iter().any(|e| matches!(e, AgentEvent::ToolFailed { .. }));

    assert!(has_approval_denied, "expected ApprovalDenied event; got: {events:?}");
    assert!(has_tool_failed, "expected ToolFailed event; got: {events:?}");
}

// ── M. Security hardening — additional approval enforcement tests ─────────────

/// DenyAbove(Critical): High < Critical, so write_file (High) must be APPROVED.
#[tokio::test]
async fn deny_above_critical_approves_high() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("new_file.txt");
    let reg = make_registry(make_policy(&dir), None);
    let runner = ToolRunner::new(reg, Duration::from_secs(30))
        .with_approval(Arc::new(DenyAbove { threshold: ToolSensitivity::Critical }));

    let result = runner
        .execute_simple(
            ToolCall {
                id: "m-crit".into(),
                name: "write_file".into(),
                arguments: json!({ "path": file_path.to_string_lossy(), "content": "hello" }),
            },
            fresh_run_id(),
        )
        .await;

    assert!(
        result.is_ok(),
        "DenyAbove(Critical) must approve High-sensitivity write_file; got: {result:?}"
    );
}

/// DenyAbove(High): edit_file is High sensitivity → must be DENIED.
#[tokio::test]
async fn runner_with_deny_all_blocks_high_tools() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("edit_target.txt");
    std::fs::write(&file_path, "original").unwrap();

    let reg = make_registry(make_policy(&dir), None);
    let runner = ToolRunner::new(reg, Duration::from_secs(30))
        .with_approval(Arc::new(DenyAbove { threshold: ToolSensitivity::High }));

    let err = runner
        .execute_simple(
            ToolCall {
                id: "m-high".into(),
                name: "edit_file".into(),
                arguments: json!({
                    "path": file_path.to_string_lossy(),
                    "old_str": "original",
                    "new_str": "modified"
                }),
            },
            fresh_run_id(),
        )
        .await
        .unwrap_err();

    assert!(
        matches!(err, ToolError::PolicyDenied(_)),
        "DenyAbove(High) must block edit_file (High sensitivity); got: {err:?}"
    );
}

/// AllowList(["read_file"]): read_file (Safe, bypasses hook) is allowed;
/// write_file (High, hook invoked, not in list) is denied.
#[tokio::test]
async fn allow_list_approves_only_listed_tools() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("listed.txt");
    std::fs::write(&file_path, "content").unwrap();

    let reg = make_registry(make_policy(&dir), None);
    let runner = ToolRunner::new(reg, Duration::from_secs(30))
        .with_approval(Arc::new(AllowList::new(["read_file"])));

    // read_file is Safe → bypasses approval hook entirely → allowed
    let read_result = runner
        .execute_simple(
            ToolCall {
                id: "al-read".into(),
                name: "read_file".into(),
                arguments: json!({ "path": file_path.to_string_lossy() }),
            },
            fresh_run_id(),
        )
        .await;
    assert!(read_result.is_ok(), "read_file must be allowed; got: {read_result:?}");

    // write_file is High → hook invoked → not in AllowList → PolicyDenied
    let write_result = runner
        .execute_simple(
            ToolCall {
                id: "al-write".into(),
                name: "write_file".into(),
                arguments: json!({ "path": file_path.to_string_lossy(), "content": "new" }),
            },
            fresh_run_id(),
        )
        .await;
    assert!(
        matches!(write_result, Err(ToolError::PolicyDenied(_))),
        "write_file not in allow-list must be denied; got: {write_result:?}"
    );
}

// ── N. Overwrite safeguard integration tests ──────────────────────────────────

/// DenyExisting policy on a NEW file path → must succeed (new file is not an overwrite).
#[tokio::test]
async fn write_tool_allow_new_file_with_deny_policy() {
    let dir = TempDir::new().unwrap();
    let new_file = dir.path().join("brand_new.txt");

    let mut reg = ToolRegistry::default();
    reg.register(Arc::new(WriteFileTool::new_with_policy(
        make_policy(&dir),
        OverwritePolicy::DenyExisting,
    )));
    let runner = ToolRunner::new(Arc::new(reg), Duration::from_secs(30));

    let result = runner
        .execute_simple(
            ToolCall {
                id: "ow-new".into(),
                name: "write_file".into(),
                arguments: json!({ "path": new_file.to_string_lossy(), "content": "fresh" }),
            },
            fresh_run_id(),
        )
        .await;

    assert!(result.is_ok(), "DenyExisting allows new file; got: {result:?}");
    assert_eq!(std::fs::read_to_string(&new_file).unwrap(), "fresh");
}

/// RequireConfirmation policy, existing file, explicit `overwrite: true` → must succeed.
#[tokio::test]
async fn write_tool_overwrite_confirmed_flag_works() {
    let dir = TempDir::new().unwrap();
    let existing = dir.path().join("existing.txt");
    std::fs::write(&existing, "original").unwrap();

    let mut reg = ToolRegistry::default();
    reg.register(Arc::new(WriteFileTool::new_with_policy(
        make_policy(&dir),
        OverwritePolicy::RequireConfirmation,
    )));
    let runner = ToolRunner::new(Arc::new(reg), Duration::from_secs(30));

    let result = runner
        .execute_simple(
            ToolCall {
                id: "ow-confirm".into(),
                name: "write_file".into(),
                arguments: json!({
                    "path": existing.to_string_lossy(),
                    "content": "updated",
                    "overwrite": true
                }),
            },
            fresh_run_id(),
        )
        .await;

    assert!(result.is_ok(), "RequireConfirmation with overwrite:true must succeed; got: {result:?}");
    assert_eq!(std::fs::read_to_string(&existing).unwrap(), "updated");
}
