//! `rustpi-tui` entry point — Phase 11 Ratatui TUI.

use anyhow::Result;
use cli::executor::Executor;
use config_core::loader::ConfigLoader;
use std::path::PathBuf;
use tui::app::App;

#[tokio::main]
async fn main() -> Result<()> {
    // Log to stderr so it doesn't corrupt the terminal
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .init();

    // Respect RUSTPI_CONFIG env var for an explicit config path
    let mut loader = ConfigLoader::new();
    if let Ok(path) = std::env::var("RUSTPI_CONFIG") {
        loader = loader.with_user_path(PathBuf::from(path));
    }
    let config = loader.load().unwrap_or_default();

    // Build a fully wired executor (providers, session store, memory) from config
    let executor = Executor::new_with_config_and_persistence(&config).await;
    let server_state = executor.state;

    let app = App::new(config, server_state);
    app.run().await
}
