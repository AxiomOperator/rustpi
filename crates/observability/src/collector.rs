use crate::{
    provider::{classify_error, ProviderMetrics},
    summary::TelemetrySummary,
    tokens::TokenUsageTracker,
    tools::ToolMetrics,
};
use agent_core::{bus::EventBus, types::AgentEvent};
use chrono::Utc;
use std::sync::Arc;

/// Central telemetry collector. Consumes AgentEvents from the EventBus
/// and updates internal metrics atomically.
pub struct TelemetryCollector {
    pub provider: Arc<ProviderMetrics>,
    pub tokens: Arc<TokenUsageTracker>,
    pub tools: Arc<ToolMetrics>,
    started_at: chrono::DateTime<chrono::Utc>,
}

impl TelemetryCollector {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            provider: Arc::new(ProviderMetrics::default()),
            tokens: Arc::new(TokenUsageTracker::default()),
            tools: Arc::new(ToolMetrics::default()),
            started_at: Utc::now(),
        })
    }

    /// Subscribe to an EventBus and spawn a background task that processes events.
    pub fn subscribe_to_bus(&self, bus: &EventBus) -> tokio::task::JoinHandle<()> {
        let mut rx = bus.subscribe();
        let provider = Arc::clone(&self.provider);
        let tokens = Arc::clone(&self.tokens);
        let tools = Arc::clone(&self.tools);
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => apply_event(&provider, &tokens, &tools, event),
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("telemetry: lagged by {} events", n);
                    }
                }
            }
        })
    }

    /// Apply an event directly (for testing and for streaming from event log).
    pub fn apply(&self, event: &AgentEvent) {
        apply_event(&self.provider, &self.tokens, &self.tools, event.clone());
    }

    /// Snapshot the current telemetry as a summary.
    pub fn snapshot(&self) -> TelemetrySummary {
        TelemetrySummary {
            collected_since: self.started_at,
            snapshot_at: Utc::now(),
            providers: self.provider.snapshot(),
            tokens: self.tokens.snapshot(),
            tools: self.tools.snapshot(),
        }
    }
}

impl Default for TelemetryCollector {
    fn default() -> Self {
        Self {
            provider: Arc::new(ProviderMetrics::default()),
            tokens: Arc::new(TokenUsageTracker::default()),
            tools: Arc::new(ToolMetrics::default()),
            started_at: Utc::now(),
        }
    }
}

fn apply_event(
    provider: &ProviderMetrics,
    tokens: &TokenUsageTracker,
    tools: &ToolMetrics,
    event: AgentEvent,
) {
    match event {
        AgentEvent::RunStarted {
            provider: p, ..
        } => {
            provider.record_run_start(&p.0);
        }
        AgentEvent::RunCompleted { .. } => {
            // Latency would be tracked with RunStarted timestamp in a stateful impl.
        }
        AgentEvent::RunFailed { reason, .. } => {
            provider.record_error(&"unknown".to_string(), classify_error(&reason));
        }
        AgentEvent::RunCancelled { .. } => {
            provider.record_cancellation();
        }
        AgentEvent::TokenChunk { delta, run_id, .. } => {
            tokens.record_chunk(&run_id.to_string(), delta.len() as u32);
        }
        AgentEvent::ContextBuilt {
            run_id,
            token_count,
            ..
        } => {
            tokens.record_context(&run_id.to_string(), token_count);
        }
        AgentEvent::ToolExecutionFailed { call_id, reason, .. } => {
            // ToolExecutionFailed has no tool_name field; use call_id as identifier.
            tools.record_failure(&call_id, &reason);
        }
        AgentEvent::ToolFailed {
            tool_name, reason, ..
        } => {
            tools.record_failure(&tool_name, &reason);
        }
        AgentEvent::ToolExecutionCancelled { .. } | AgentEvent::ToolCancelled { .. } => {
            tools.record_cancellation();
        }
        AgentEvent::AuthLoginFailed { provider: p, reason, .. } => {
            provider.record_error(&p.0, classify_error(&reason));
        }
        AgentEvent::TokenRefreshFailed { provider: p, reason, .. } => {
            provider.record_error(&p.0, classify_error(&reason));
        }
        _ => {}
    }
}
