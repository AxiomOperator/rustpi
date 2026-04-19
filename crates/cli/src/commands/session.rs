//! `rustpi session` subcommands.

use crate::{
    args::SessionCommand,
    error::{CliError, CliResult},
    executor::{parse_session_id, Executor},
    output::Output,
};

pub async fn session_command(
    subcommand: SessionCommand,
    output: &Output,
    executor: &Executor,
) -> CliResult<()> {
    match subcommand {
        SessionCommand::List => list(output, executor),
        SessionCommand::Attach { id } => attach(id, output, executor).await,
        SessionCommand::Detach { id } => detach(id, output, executor).await,
        SessionCommand::Info { id } => info(id, output, executor),
    }
}

fn list(output: &Output, executor: &Executor) -> CliResult<()> {
    let sessions = executor.session_list()?;

    if output.format == crate::args::OutputFormat::Json {
        let data = serde_json::to_value(&sessions)
            .map_err(|e| CliError::Other(e.to_string()))?;
        output.print_success("", &data);
        return Ok(());
    }

    output.print_header(&format!("Sessions ({} in memory)", sessions.len()));
    if sessions.is_empty() {
        println!("  (none)");
        println!();
        println!("  Note: sessions are in-memory only and are lost when the process exits.");
    } else {
        println!("  {:<38}  {:<8}  {:>4}  CREATED", "ID", "STATUS", "RUNS");
        println!("  {}", "─".repeat(70));
        for s in &sessions {
            println!(
                "  {:<38}  {:<8}  {:>4}  {}",
                s.session_id, s.status, s.run_count, s.created_at
            );
            if let Some(label) = &s.label {
                println!("  {:<38}  label: {}", "", label);
            }
        }
    }
    output.print_blank();
    Ok(())
}

async fn attach(id: Option<String>, output: &Output, executor: &Executor) -> CliResult<()> {
    let session_id = id.as_deref().map(parse_session_id).transpose()?;
    let action = if session_id.is_some() { "Attaching to" } else { "Creating new" };
    output.print_info(&format!("{} session…", action));

    let info = executor.session_attach(session_id).await?;

    if output.format == crate::args::OutputFormat::Json {
        let data = serde_json::to_value(&info)
            .map_err(|e| CliError::Other(e.to_string()))?;
        output.print_success("", &data);
    } else {
        output.print_header("Session");
        output.print_kv("id", &info.session_id);
        output.print_kv("status", &info.status);
        output.print_kv("runs", &info.run_count.to_string());
        output.print_kv("created_at", &info.created_at);
        if let Some(label) = &info.label {
            output.print_kv("label", label);
        }
        output.print_blank();
    }
    Ok(())
}

async fn detach(id: String, output: &Output, executor: &Executor) -> CliResult<()> {
    let session_id = parse_session_id(&id)?;
    executor.session_detach(session_id).await?;

    if output.format == crate::args::OutputFormat::Json {
        output.print_success("", &serde_json::json!({"detached": true, "session_id": id}));
    } else {
        println!("Session {} detached.", id);
    }
    Ok(())
}

fn info(id: String, output: &Output, executor: &Executor) -> CliResult<()> {
    let session_id = parse_session_id(&id)?;
    let info = executor.session_info(&session_id)?;

    if output.format == crate::args::OutputFormat::Json {
        let data = serde_json::to_value(&info)
            .map_err(|e| CliError::Other(e.to_string()))?;
        output.print_success("", &data);
    } else {
        output.print_header("Session Info");
        output.print_kv("id", &info.session_id);
        output.print_kv("status", &info.status);
        output.print_kv("runs", &info.run_count.to_string());
        output.print_kv("created_at", &info.created_at);
        if let Some(label) = &info.label {
            output.print_kv("label", label);
        }
        output.print_blank();
    }
    Ok(())
}
