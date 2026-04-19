//! Core subprocess executor.
//!
//! Provides [`SubprocessExecutor`] which spawns a child process and:
//! - Streams stdout and stderr lines as [`AgentEvent`] on the provided channel
//! - Enforces a wall-clock timeout (kills process on expiry)
//! - Supports cancellation via [`CancellationToken`]
//! - Captures up to `MAX_OUTPUT_BYTES` of each stream to include in the final result
//! - Returns a [`SubprocessResult`] with exit code, captured output, and termination reason

use agent_core::{types::{AgentEvent, RunId}, Redactor};
use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

/// Maximum bytes captured per stream (stdout / stderr). Lines beyond this are dropped.
pub const MAX_OUTPUT_BYTES: usize = 512 * 1024; // 512 KB

/// How the subprocess ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminationReason {
    /// Process exited on its own.
    Exited,
    /// Killed because the timeout was reached.
    TimedOut,
    /// Killed because cancellation was requested.
    Cancelled,
}

/// Result of a subprocess execution.
#[derive(Debug, Clone)]
pub struct SubprocessResult {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub reason: TerminationReason,
}

impl SubprocessResult {
    pub fn success(&self) -> bool {
        self.exit_code == Some(0) && self.reason == TerminationReason::Exited
    }
}

/// Configuration for a subprocess execution.
pub struct SubprocessConfig {
    /// Program to run.
    pub program: String,
    /// Arguments.
    pub args: Vec<String>,
    /// Working directory. Defaults to current directory.
    pub working_dir: Option<std::path::PathBuf>,
    /// Environment variable overrides.
    pub env: Vec<(String, String)>,
    /// Wall-clock timeout. Process is killed on expiry.
    pub timeout: Duration,
    /// Optional cancellation token.
    pub cancel: Option<CancellationToken>,
    /// Optional event channel for streaming stdout/stderr lines.
    pub event_tx: Option<broadcast::Sender<AgentEvent>>,
    /// RunId context for event payloads. Required if event_tx is set.
    pub run_id: Option<RunId>,
    /// call_id context for event payloads.
    pub call_id: Option<String>,
    /// Optional redactor applied to each captured line before emitting events and
    /// storing to captured output.
    pub redactor: Option<Arc<Redactor>>,
}

/// Spawn a subprocess and run it to completion (or timeout/cancellation).
pub async fn run_subprocess(config: SubprocessConfig) -> std::io::Result<SubprocessResult> {
    let mut cmd = Command::new(&config.program);
    cmd.args(&config.args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    if let Some(dir) = &config.working_dir {
        cmd.current_dir(dir);
    }
    for (k, v) in &config.env {
        cmd.env(k, v);
    }

    let mut child = cmd.spawn()?;

    let stdout_pipe = child.stdout.take().expect("stdout captured");
    let stderr_pipe = child.stderr.take().expect("stderr captured");

    let run_id = config.run_id.clone();
    let call_id = config.call_id.clone();
    let event_tx_stdout = config.event_tx.clone();
    let event_tx_stderr = config.event_tx.clone();
    let run_id_stderr = run_id.clone();
    let call_id_stderr = call_id.clone();
    let redactor_stdout = config.redactor.clone();
    let redactor_stderr = config.redactor.clone();

    // Spawn stdout reader task
    let stdout_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout_pipe).lines();
        let mut captured = String::new();
        while let Ok(Some(line)) = reader.next_line().await {
            let line = match &redactor_stdout {
                Some(r) => r.redact(&line),
                None => line,
            };
            if captured.len() < MAX_OUTPUT_BYTES {
                captured.push_str(&line);
                captured.push('\n');
            }
            if let (Some(tx), Some(rid), Some(cid)) =
                (&event_tx_stdout, &run_id, &call_id)
            {
                let _ = tx.send(AgentEvent::ToolStdout {
                    run_id: rid.clone(),
                    call_id: cid.clone(),
                    line,
                    timestamp: Utc::now(),
                });
            }
        }
        captured
    });

    // Spawn stderr reader task
    let stderr_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr_pipe).lines();
        let mut captured = String::new();
        while let Ok(Some(line)) = reader.next_line().await {
            let line = match &redactor_stderr {
                Some(r) => r.redact(&line),
                None => line,
            };
            if captured.len() < MAX_OUTPUT_BYTES {
                captured.push_str(&line);
                captured.push('\n');
            }
            if let (Some(tx), Some(rid), Some(cid)) =
                (&event_tx_stderr, &run_id_stderr, &call_id_stderr)
            {
                let _ = tx.send(AgentEvent::ToolStderr {
                    run_id: rid.clone(),
                    call_id: cid.clone(),
                    line,
                    timestamp: Utc::now(),
                });
            }
        }
        captured
    });

    // Race: process exit vs timeout vs cancellation
    let reason;
    let exit_code;

    let timeout_fut = tokio::time::sleep(config.timeout);
    tokio::pin!(timeout_fut);

    if let Some(cancel) = config.cancel {
        tokio::select! {
            status = child.wait() => {
                exit_code = status.ok().and_then(|s| s.code());
                reason = TerminationReason::Exited;
            }
            _ = &mut timeout_fut => {
                let _ = child.kill().await;
                exit_code = None;
                reason = TerminationReason::TimedOut;
            }
            _ = cancel.cancelled() => {
                let _ = child.kill().await;
                exit_code = None;
                reason = TerminationReason::Cancelled;
            }
        }
    } else {
        tokio::select! {
            status = child.wait() => {
                exit_code = status.ok().and_then(|s| s.code());
                reason = TerminationReason::Exited;
            }
            _ = &mut timeout_fut => {
                let _ = child.kill().await;
                exit_code = None;
                reason = TerminationReason::TimedOut;
            }
        }
    }

    // Collect streamed output
    let stdout = stdout_task.await.unwrap_or_default();
    let stderr = stderr_task.await.unwrap_or_default();

    Ok(SubprocessResult { exit_code, stdout, stderr, reason })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn echo_command_succeeds() {
        let result = run_subprocess(SubprocessConfig {
            program: "echo".into(),
            args: vec!["hello world".into()],
            working_dir: None,
            env: vec![],
            timeout: Duration::from_secs(5),
            cancel: None,
            event_tx: None,
            run_id: None,
            call_id: None,
            redactor: None,
        })
        .await
        .unwrap();
        assert_eq!(result.exit_code, Some(0));
        assert_eq!(result.reason, TerminationReason::Exited);
        assert!(result.stdout.contains("hello world"));
        assert!(result.success());
    }

    #[tokio::test]
    async fn nonzero_exit_is_captured() {
        let result = run_subprocess(SubprocessConfig {
            program: "sh".into(),
            args: vec!["-c".into(), "exit 42".into()],
            working_dir: None,
            env: vec![],
            timeout: Duration::from_secs(5),
            cancel: None,
            event_tx: None,
            run_id: None,
            call_id: None,
            redactor: None,
        })
        .await
        .unwrap();
        assert_eq!(result.exit_code, Some(42));
        assert!(!result.success());
        assert_eq!(result.reason, TerminationReason::Exited);
    }

    #[tokio::test]
    async fn timeout_kills_process() {
        let result = run_subprocess(SubprocessConfig {
            program: "sleep".into(),
            args: vec!["10".into()],
            working_dir: None,
            env: vec![],
            timeout: Duration::from_millis(200),
            cancel: None,
            event_tx: None,
            run_id: None,
            call_id: None,
            redactor: None,
        })
        .await
        .unwrap();
        assert_eq!(result.reason, TerminationReason::TimedOut);
        assert!(!result.success());
    }

    #[tokio::test]
    async fn cancellation_kills_process() {
        let token = CancellationToken::new();
        let token_clone = token.clone();

        // Cancel after a short delay
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            token_clone.cancel();
        });

        let result = run_subprocess(SubprocessConfig {
            program: "sleep".into(),
            args: vec!["10".into()],
            working_dir: None,
            env: vec![],
            timeout: Duration::from_secs(30),
            cancel: Some(token),
            event_tx: None,
            run_id: None,
            call_id: None,
            redactor: None,
        })
        .await
        .unwrap();
        assert_eq!(result.reason, TerminationReason::Cancelled);
        assert!(!result.success());
    }

    #[tokio::test]
    async fn stdout_is_streamed_as_events() {
        let (tx, mut rx) = broadcast::channel(32);
        let result = run_subprocess(SubprocessConfig {
            program: "echo".into(),
            args: vec!["streamed line".into()],
            working_dir: None,
            env: vec![],
            timeout: Duration::from_secs(5),
            cancel: None,
            event_tx: Some(tx),
            run_id: Some(agent_core::types::RunId::new()),
            call_id: Some("test-call".into()),
            redactor: None,
        })
        .await
        .unwrap();
        assert!(result.success());
        // Should have received a ToolStdout event
        let event = rx.try_recv().unwrap();
        assert!(matches!(event, AgentEvent::ToolStdout { .. }));
    }
}
