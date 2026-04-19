//! Failure-mode tests for the tool runtime.
//!
//! Covers:
//! - Shell tool timeout via the ToolRunner path
//! - Nonexistent program returning an I/O error (not a panic) via the
//!   subprocess executor directly

use std::sync::Arc;
use std::time::Duration;

use agent_core::types::{RunId, ToolCall};
use serde_json::json;
use tempfile::TempDir;

use tool_runtime::{
    path_safety::PathSafetyPolicy,
    registry::ToolRegistry,
    runner::ToolRunner,
    schema::ToolConfig,
    subprocess::{run_subprocess, SubprocessConfig},
    tools::{
        file::{ReadFileTool, WriteFileTool},
        shell::ShellTool,
    },
    ToolError,
};

fn make_policy(dir: &TempDir) -> Arc<PathSafetyPolicy> {
    Arc::new(PathSafetyPolicy::new([dir.path()]))
}

fn runner_in(dir: &TempDir) -> ToolRunner {
    let policy = make_policy(dir);
    let mut reg = ToolRegistry::default();
    reg.register(Arc::new(ShellTool::new()));
    reg.register(Arc::new(ReadFileTool::new(policy.clone())));
    reg.register(Arc::new(WriteFileTool::new(policy.clone())));
    ToolRunner::new(Arc::new(reg), Duration::from_secs(30))
}

fn fresh_run_id() -> RunId {
    RunId::new()
}

/// Running a long command via ShellTool with a short per-call `timeout_secs`
/// override must return `ToolError::Timeout` promptly — not hang.
#[tokio::test]
async fn shell_tool_timeout_cancels_and_returns_error() {
    let dir = TempDir::new().unwrap();
    let runner = runner_in(&dir);

    let call = ToolCall {
        id: "fm-t1".into(),
        name: "shell".into(),
        // timeout_secs: 0 makes the ShellTool's own subprocess timeout fire immediately.
        arguments: json!({ "command": "sleep 10", "timeout_secs": 0 }),
    };
    let result = runner
        .execute(
            call,
            ToolConfig {
                run_id: Some(fresh_run_id()),
                ..Default::default()
            },
        )
        .await;

    assert!(
        matches!(result, Err(ToolError::Timeout(_))),
        "expected Timeout error for a hung shell command, got: {result:?}"
    );
}

/// Running a command that does not exist via the underlying subprocess executor
/// must return an I/O error — not panic, not silently succeed.
#[tokio::test]
async fn shell_tool_nonexistent_command_returns_tool_error() {
    // The ShellTool always shells out via `sh -c`, so a missing *shell command*
    // yields exit code 127 (clean result, not an error).  To exercise the true
    // "program not found" I/O error path we call `run_subprocess` directly with
    // a non-existent binary name.
    let result = run_subprocess(SubprocessConfig {
        program: "definitely_nonexistent_program_xyz_rustpi".into(),
        args: vec![],
        working_dir: None,
        env: vec![],
        timeout: Duration::from_secs(5),
        cancel: None,
        event_tx: None,
        run_id: None,
        call_id: None,
        redactor: None,
    })
    .await;

    assert!(
        result.is_err(),
        "spawning a non-existent program must return an I/O error, not Ok"
    );
    let io_err = result.unwrap_err();
    assert_eq!(
        io_err.kind(),
        std::io::ErrorKind::NotFound,
        "expected NotFound I/O error, got: {io_err}"
    );
}
