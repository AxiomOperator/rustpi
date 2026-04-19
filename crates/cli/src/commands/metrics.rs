//! `rustpi metrics` — display the current telemetry snapshot.

use std::sync::Arc;

use observability::TelemetryCollector;

use crate::{error::CliResult, output::Output};

pub async fn metrics_command(collector: &Arc<TelemetryCollector>, output: &Output) -> CliResult<()> {
    let summary = collector.snapshot();
    let json = serde_json::to_string_pretty(&summary)
        .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"));
    output.print_info(&json);
    Ok(())
}
