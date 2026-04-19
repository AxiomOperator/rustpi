//! `rustpi run` — submit a prompt and stream the response.

use std::io::{self, IsTerminal, Read};

use agent_core::types::{ModelId, ProviderId, SessionId};
use rpc_api::protocol::RpcResponse;

use crate::{
    args::{OutputFormat, RunArgs},
    error::{CliError, CliResult},
    executor::Executor,
    output::Output,
};

pub async fn run_command(
    args: &RunArgs,
    provider: Option<ProviderId>,
    model: Option<ModelId>,
    session_id: Option<SessionId>,
    non_interactive: bool,
    output: &Output,
    executor: &Executor,
) -> CliResult<()> {
    let prompt = resolve_prompt(args, non_interactive)?;

    if matches!(output.format, OutputFormat::Print) {
        run_print_mode(prompt, provider, model, session_id, output, executor).await
    } else {
        run_json_mode(prompt, provider, model, session_id, output, executor).await
    }
}

/// Stream tokens directly to stdout as they arrive.
async fn run_print_mode(
    prompt: String,
    provider: Option<ProviderId>,
    model: Option<ModelId>,
    session_id: Option<SessionId>,
    output: &Output,
    executor: &Executor,
) -> CliResult<()> {
    let mut run_error: Option<String> = None;

    executor
        .run_start(session_id, provider, model, prompt, |resp| match resp {
            RpcResponse::StreamEvent { event } => match event.event_type.as_str() {
                "token_chunk" => {
                    if let Some(delta) =
                        event.payload.get("delta").and_then(|v| v.as_str())
                    {
                        output.print_token(delta);
                    }
                    true
                }
                "run_completed" => false,
                "run_failed" => {
                    run_error = Some(
                        event
                            .payload
                            .get("reason")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown reason")
                            .to_string(),
                    );
                    false
                }
                "run_cancelled" => {
                    run_error = Some("run was cancelled".to_string());
                    false
                }
                other => {
                    output.print_info(other);
                    true
                }
            },
            RpcResponse::Ack { .. } => true,
            RpcResponse::Error { message, .. } => {
                run_error = Some(message);
                false
            }
            _ => true,
        })
        .await?;

    // Ensure the cursor is on a new line after streaming.
    println!();

    match run_error {
        Some(e) => Err(CliError::RunFailed(e)),
        None => Ok(()),
    }
}

/// Collect all token chunks and emit JSONL, finishing with a `done` event.
async fn run_json_mode(
    prompt: String,
    provider: Option<ProviderId>,
    model: Option<ModelId>,
    session_id: Option<SessionId>,
    output: &Output,
    executor: &Executor,
) -> CliResult<()> {
    let mut token_buf = String::new();
    let mut run_error: Option<String> = None;

    executor
        .run_start(session_id, provider, model, prompt, |resp| match resp {
            RpcResponse::StreamEvent { event } => match event.event_type.as_str() {
                "token_chunk" => {
                    if let Some(delta) =
                        event.payload.get("delta").and_then(|v| v.as_str())
                    {
                        output.emit_json_line(
                            "token_chunk",
                            serde_json::json!({ "delta": delta }),
                        );
                        token_buf.push_str(delta);
                    }
                    true
                }
                "run_completed" => false,
                "run_failed" => {
                    run_error = Some(
                        event
                            .payload
                            .get("reason")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown reason")
                            .to_string(),
                    );
                    false
                }
                "run_cancelled" => {
                    run_error = Some("run was cancelled".to_string());
                    false
                }
                _ => true,
            },
            RpcResponse::Ack { .. } => true,
            RpcResponse::Error { message, .. } => {
                run_error = Some(message);
                false
            }
            _ => true,
        })
        .await?;

    match run_error {
        Some(e) => Err(CliError::RunFailed(e)),
        None => {
            output.emit_json_done(serde_json::json!({ "output": token_buf }));
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// Prompt resolution
// ---------------------------------------------------------------------------

/// Resolve the prompt text from: `--file` > positional arg > piped stdin > error.
fn resolve_prompt(args: &RunArgs, non_interactive: bool) -> CliResult<String> {
    if let Some(path) = &args.file {
        return std::fs::read_to_string(path).map_err(CliError::Io);
    }

    if let Some(prompt) = &args.prompt {
        return Ok(prompt.clone());
    }

    // Read from piped stdin if stdin is not a TTY.
    if !io::stdin().is_terminal() {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf).map_err(CliError::Io)?;
        let trimmed = buf.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
    }

    if non_interactive {
        return Err(CliError::InvalidArgs(
            "--non-interactive: no prompt provided".into(),
        ));
    }

    Err(CliError::InvalidArgs(
        "no prompt provided; supply a positional argument, --file <PATH>, or pipe via stdin".into(),
    ))
}
