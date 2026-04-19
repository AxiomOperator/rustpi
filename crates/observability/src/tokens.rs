use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Default)]
pub struct TokenUsageTracker {
    inner: Mutex<TokenUsageInner>,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct TokenUsageInner {
    /// Total estimated tokens (from chunk deltas) per run_id.
    pub per_run: HashMap<String, TokenUsageRecord>,
    /// Total across all runs.
    pub total_estimated: u64,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct TokenUsageRecord {
    pub run_id: String,
    pub session_id: Option<String>,
    pub provider_id: Option<String>,
    /// Sum of delta lengths (estimated, not exact unless provider reports).
    pub estimated_output_tokens: u32,
    /// Estimated input tokens from ContextBuilt events.
    pub estimated_input_tokens: u32,
    /// Whether this is exact (provider-reported) or estimated.
    pub is_estimated: bool,
    pub timestamp: DateTime<Utc>,
}

impl TokenUsageTracker {
    pub fn record_chunk(&self, run_id: &str, delta_len: u32) {
        let mut g = self.inner.lock().unwrap();
        let rec = g.per_run.entry(run_id.to_string()).or_insert_with(|| TokenUsageRecord {
            run_id: run_id.to_string(),
            is_estimated: true,
            timestamp: Utc::now(),
            ..Default::default()
        });
        rec.estimated_output_tokens += delta_len;
        g.total_estimated += delta_len as u64;
    }

    pub fn record_context(&self, run_id: &str, token_count: u32) {
        let mut g = self.inner.lock().unwrap();
        let rec = g.per_run.entry(run_id.to_string()).or_insert_with(|| TokenUsageRecord {
            run_id: run_id.to_string(),
            is_estimated: true,
            timestamp: Utc::now(),
            ..Default::default()
        });
        rec.estimated_input_tokens += token_count;
    }

    pub fn total_estimated_tokens(&self) -> u64 {
        self.inner.lock().unwrap().total_estimated
    }

    pub fn for_run(&self, run_id: &str) -> Option<TokenUsageRecord> {
        self.inner.lock().unwrap().per_run.get(run_id).cloned()
    }

    pub fn snapshot(&self) -> TokenUsageInner {
        self.inner.lock().unwrap().clone()
    }
}
