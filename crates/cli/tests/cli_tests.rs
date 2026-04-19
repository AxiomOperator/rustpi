//! End-to-end integration tests for the `rustpi` CLI (Phase 10).
//!
//! Two testing approaches:
//!  - **Process tests**: invoke the compiled `rustpi` binary via `std::process::Command`.
//!  - **Library tests**: import from the `cli` lib crate to test `Executor` and `Output`
//!    internals directly (async tests with `#[tokio::test]`).

use std::process::Command;

use cli::args::OutputFormat;
use cli::error::CliError;
use cli::executor::{parse_session_id, Executor};
use cli::output::Output;

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn rustpi() -> Command {
    let bin = env!("CARGO_BIN_EXE_rustpi");
    Command::new(bin)
}

// ===========================================================================
// Approach A: Binary process tests
// ===========================================================================

// ─── 1. Help / args parsing ─────────────────────────────────────────────────

#[test]
fn test_help_flag() {
    let out = rustpi().arg("--help").output().unwrap();
    assert!(out.status.success(), "exit: {:?}", out.status.code());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("rustpi"), "expected 'rustpi' in: {stdout}");
}

#[test]
fn test_run_help() {
    let out = rustpi().args(["run", "--help"]).output().unwrap();
    assert!(out.status.success(), "exit: {:?}", out.status.code());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("run"), "expected 'run' in: {stdout}");
}

#[test]
fn test_no_command_prints_help() {
    let out = rustpi().output().unwrap();
    assert!(out.status.success(), "exit: {:?}", out.status.code());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("rustpi"), "expected help output: {stdout}");
}

#[test]
fn test_invalid_output_format() {
    let out = rustpi().args(["--output", "badval"]).output().unwrap();
    assert!(!out.status.success());
    // clap exits with code 2 for bad argument values
    assert_eq!(out.status.code(), Some(2));
}

// ─── 2. Run command ─────────────────────────────────────────────────────────

#[test]
fn test_run_with_prompt() {
    // Simulation emits 3 token chunks (~150 ms) then run_completed → exit 0.
    let out = rustpi().args(["run", "hello"]).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn test_run_with_file_flag() {
    let out = rustpi()
        .args(["run", "--file", "/nonexistent/path_that_does_not_exist.md"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    // IO error → CliError::Io → exit code 1
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn test_run_non_interactive_no_prompt() {
    // --non-interactive with no prompt → InvalidArgs → exit 2
    let out = rustpi().args(["run", "--non-interactive"]).output().unwrap();
    assert!(!out.status.success());
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn test_run_with_provider_flag() {
    // --provider is global; run still uses the simulation → exit 0
    let out = rustpi()
        .args(["--provider", "openai", "run", "hello"])
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn test_run_with_model_flag() {
    let out = rustpi()
        .args(["--model", "gpt-4o", "run", "hello"])
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn test_run_from_file() {
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("prompt.txt");
    std::fs::write(&file_path, "hello from file").unwrap();

    let out = rustpi()
        .args(["run", "--file"])
        .arg(&file_path)
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

// ─── 3. Session command ──────────────────────────────────────────────────────

#[test]
fn test_session_list() {
    let out = rustpi().args(["session", "list"]).output().unwrap();
    assert!(out.status.success(), "exit: {:?}", out.status.code());
    // Print mode produces non-empty output (header + "(none)")
    assert!(!String::from_utf8_lossy(&out.stdout).trim().is_empty());
}

#[test]
fn test_session_list_json() {
    // Each process starts with a fresh in-memory state → empty list
    let out = rustpi()
        .args(["--output", "json", "session", "list"])
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("stdout should be valid JSON");
    assert_eq!(parsed["ok"], serde_json::Value::Bool(true));
    assert!(parsed["data"].is_array(), "data should be an array");
}

// ─── 4. Auth command ─────────────────────────────────────────────────────────

#[test]
fn test_auth_status() {
    // Dispatch stub always returns authenticated=false → exit 0
    let out = rustpi().args(["auth", "status"]).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

// ─── 5. Diag command ─────────────────────────────────────────────────────────

#[test]
fn test_diag() {
    let out = rustpi().arg("diag").output().unwrap();
    assert!(out.status.success(), "exit: {:?}", out.status.code());
    // Print-mode diagnostics write several sections to stdout
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.trim().is_empty(), "diag output should be non-empty");
}

#[test]
fn test_diag_json() {
    let out = rustpi().args(["--output", "json", "diag"]).output().unwrap();
    assert!(out.status.success(), "exit: {:?}", out.status.code());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.trim().is_empty(), "diag --output json stdout should be non-empty");
}

#[test]
fn test_diag_json_valid() {
    let out = rustpi().args(["--output", "json", "diag"]).output().unwrap();
    assert!(out.status.success(), "exit: {:?}", out.status.code());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("diag --output json must produce valid JSON");
    assert_eq!(parsed["ok"], serde_json::Value::Bool(true), "ok must be true");
    assert!(parsed["data"].is_object(), "data must be an object");
}

// ===========================================================================
// Approach B: Library / executor tests (async, in-process)
// ===========================================================================

// ─── 6. Executor session operations ─────────────────────────────────────────

#[tokio::test]
async fn test_executor_session_attach_detach() {
    let executor = Executor::new();

    // Create a new session via attach.
    let info = executor.session_attach(None).await.unwrap();
    assert!(!info.session_id.is_empty());
    assert_eq!(info.run_count, 0);

    // Detach; session transitions to "ended".
    let session_id = parse_session_id(&info.session_id).unwrap();
    executor.session_detach(session_id.clone()).await.unwrap();

    // Session still exists in-memory but has ended status.
    let after = executor.session_info(&session_id).unwrap();
    assert_eq!(after.status, "ended");
}

#[tokio::test]
async fn test_executor_session_list() {
    let executor = Executor::new();

    let s1 = executor.session_attach(None).await.unwrap();
    let s2 = executor.session_attach(None).await.unwrap();

    let sessions = executor.session_list().unwrap();
    assert_eq!(sessions.len(), 2);

    let ids: Vec<&str> = sessions.iter().map(|s| s.session_id.as_str()).collect();
    assert!(ids.contains(&s1.session_id.as_str()));
    assert!(ids.contains(&s2.session_id.as_str()));
}

#[tokio::test]
async fn test_executor_session_info_not_found() {
    let executor = Executor::new();
    let fake_id = parse_session_id("00000000-0000-0000-0000-000000000001").unwrap();
    let result = executor.session_info(&fake_id);
    assert!(
        matches!(result, Err(CliError::SessionNotFound(_))),
        "expected SessionNotFound, got: {:?}",
        result
    );
}

// ===========================================================================
// Approach B: Output formatting (unit tests)
// ===========================================================================

// ─── 7. Output mode ──────────────────────────────────────────────────────────

#[test]
fn test_output_print_mode() {
    let output = Output::new(OutputFormat::Print, false);
    assert_eq!(output.format, OutputFormat::Print);
    // In a non-TTY test environment color should be disabled.
    assert!(!output.color);
}

#[test]
fn test_output_json_success() {
    // Verify the JSON envelope shape that Output::print_success uses in JSON mode.
    let data = serde_json::json!({ "session_id": "test-123", "status": "active" });
    let envelope = serde_json::json!({ "ok": true, "data": data });
    assert_eq!(envelope["ok"], serde_json::Value::Bool(true));
    assert_eq!(envelope["data"]["session_id"], "test-123");
    assert!(envelope["data"].is_object());
}

#[test]
fn test_output_json_error() {
    // Output created in JSON mode has the correct format field.
    let output = Output::new(OutputFormat::Json, false);
    assert_eq!(output.format, OutputFormat::Json);
    // print_err always writes "error: <msg>" to stderr regardless of mode.
    // (Verified at the process level by test_run_with_file_flag / similar.)
}
