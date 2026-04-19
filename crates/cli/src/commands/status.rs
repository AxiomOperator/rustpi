//! `rustpi status` — show configuration and provider status.

use crate::executor::Executor;
use crate::error::CliResult;

pub async fn show_status(executor: &Executor) -> CliResult<()> {
    println!("rustpi status");
    println!("─────────────────────────────");

    let provider_ids = executor.state.provider_registry.list();
    if provider_ids.is_empty() {
        println!("Providers:     none configured");
        println!("  → Add [[providers]] to ~/.config/rustpi/config.toml");
    } else {
        for id in provider_ids {
            println!("Provider:      {}", id);
        }
    }

    println!(
        "Session store: {}",
        if executor.state.session_store.is_some() {
            "SQLite (~/.rustpi/sessions.db)"
        } else {
            "in-memory (not persisted)"
        }
    );

    println!(
        "Event log:     {}",
        if executor.state.event_store.is_some() {
            "JSONL (~/.rustpi/events.jsonl)"
        } else {
            "disabled"
        }
    );

    Ok(())
}
