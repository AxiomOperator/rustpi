//! `rustpi-tui` entry point.

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    tracing::info!("rustpi TUI — Phase 0 stub. Full implementation in Phase 11.");
    println!("rustpi TUI (Phase 0 stub — not yet implemented)");
    Ok(())
}
