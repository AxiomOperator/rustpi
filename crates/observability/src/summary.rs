use crate::{provider::ProviderMetricsInner, tokens::TokenUsageInner, tools::ToolMetricsInner};
use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct TelemetrySummary {
    pub collected_since: DateTime<Utc>,
    pub snapshot_at: DateTime<Utc>,
    pub providers: ProviderMetricsInner,
    pub tokens: TokenUsageInner,
    pub tools: ToolMetricsInner,
}

impl TelemetrySummary {
    /// Render as a human-readable diagnostic string.
    pub fn to_display_string(&self) -> String {
        let total_runs: u64 = self.providers.runs_started.values().sum();
        let total_errors: u64 = self
            .providers
            .errors
            .values()
            .flat_map(|m| m.values())
            .sum();
        let total_tool_failures: u64 = self.tools.failures.values().sum();

        let mut lines = vec![
            format!(
                "Telemetry collected since: {}",
                self.collected_since.format("%Y-%m-%dT%H:%M:%SZ")
            ),
            format!(
                "Snapshot at: {}",
                self.snapshot_at.format("%Y-%m-%dT%H:%M:%SZ")
            ),
            format!("Provider runs started: {}", total_runs),
            format!("Provider errors: {}", total_errors),
            format!("Cancellations: {}", self.providers.cancellations),
            format!(
                "Estimated output tokens: {}",
                self.tokens.total_estimated
            ),
            format!("Tool failures: {}", total_tool_failures),
            format!("Tool cancellations: {}", self.tools.cancellations),
        ];

        for (provider_id, runs) in &self.providers.runs_started {
            let errors: u64 = self
                .providers
                .errors
                .get(provider_id)
                .map(|m| m.values().sum())
                .unwrap_or(0);
            let total = runs + errors;
            let rate = if total > 0 {
                errors as f64 / total as f64
            } else {
                0.0
            };
            lines.push(format!(
                "  provider={} runs={} errors={} error_rate={:.2}%",
                provider_id,
                runs,
                errors,
                rate * 100.0
            ));
        }

        lines.join("\n")
    }
}
