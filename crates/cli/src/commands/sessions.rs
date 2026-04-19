//! `rustpi sessions` — list past sessions from the persistent session store.

use crate::executor::Executor;
use crate::error::CliResult;

pub async fn list_sessions(executor: &Executor) -> CliResult<()> {
    if let Some(store) = &executor.state.session_store {
        let sessions = store
            .list_sessions()
            .await
            .map_err(|e| crate::error::CliError::Other(format!("Failed to list sessions: {}", e)))?;

        if sessions.is_empty() {
            println!("No sessions found.");
            return Ok(());
        }

        for s in &sessions {
            println!("{}", s.id);
            if let Some(summary) = &s.summary {
                let preview = if summary.len() > 80 { &summary[..80] } else { summary };
                println!("  {}", preview);
            }
            println!("  created: {}", s.created_at);
        }
    } else {
        println!("Session store not available (in-memory mode).");
    }
    Ok(())
}
