//! `rustpi` CLI entry point.

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    tracing::info!("rustpi CLI — Phase 0 stub. Full implementation in Phase 10.");
    println!("rustpi agent CLI (Phase 0 stub — not yet implemented)");
    Ok(())
}
