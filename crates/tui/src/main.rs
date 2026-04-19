//! `rustpi-tui` entry point — Phase 11 Ratatui TUI.

use anyhow::Result;
use config_core::loader::ConfigLoader;
use rpc_api::server::ServerState;
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

    let config = ConfigLoader::new().load().unwrap_or_default();
    let server_state = ServerState::new();
    let app = App::new(config, server_state);
    app.run().await
}
