use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

pub fn classify_error(reason: &str) -> &'static str {
    if reason.contains("timeout") || reason.contains("timed out") {
        "timeout"
    } else if reason.contains("auth") || reason.contains("401") || reason.contains("403") {
        "auth_failure"
    } else if reason.contains("rate") || reason.contains("429") {
        "rate_limited"
    } else if reason.contains("connect") || reason.contains("network") {
        "network"
    } else {
        "other"
    }
}

const MAX_LATENCY_SAMPLES: usize = 100;

#[derive(Debug, Default)]
pub struct ProviderMetrics {
    inner: Mutex<ProviderMetricsInner>,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct ProviderMetricsInner {
    /// Total runs started per provider id.
    pub runs_started: HashMap<String, u64>,
    /// Error counts by (provider_id, error_category).
    pub errors: HashMap<String, HashMap<String, u64>>,
    /// Total cancellations (not provider-specific).
    pub cancellations: u64,
    /// Latency samples (ms) per provider: rolling last 100.
    pub latency_samples: HashMap<String, VecDeque<u64>>,
}

impl ProviderMetrics {
    pub fn record_run_start(&self, provider_id: &str) {
        let mut g = self.inner.lock().unwrap();
        *g.runs_started.entry(provider_id.to_string()).or_insert(0) += 1;
    }

    pub fn record_error(&self, provider_id: &str, category: &str) {
        let mut g = self.inner.lock().unwrap();
        *g.errors
            .entry(provider_id.to_string())
            .or_default()
            .entry(category.to_string())
            .or_insert(0) += 1;
    }

    pub fn record_cancellation(&self) {
        let mut g = self.inner.lock().unwrap();
        g.cancellations += 1;
    }

    pub fn record_latency(&self, provider_id: &str, latency_ms: u64) {
        let mut g = self.inner.lock().unwrap();
        let samples = g
            .latency_samples
            .entry(provider_id.to_string())
            .or_default();
        if samples.len() >= MAX_LATENCY_SAMPLES {
            samples.pop_front();
        }
        samples.push_back(latency_ms);
    }

    pub fn snapshot(&self) -> ProviderMetricsInner {
        self.inner.lock().unwrap().clone()
    }

    /// Average latency for a provider (returns None if no samples).
    pub fn avg_latency_ms(&self, provider_id: &str) -> Option<f64> {
        let g = self.inner.lock().unwrap();
        let samples = g.latency_samples.get(provider_id)?;
        if samples.is_empty() {
            return None;
        }
        let sum: u64 = samples.iter().sum();
        Some(sum as f64 / samples.len() as f64)
    }

    /// Error rate for a provider: errors / (runs + errors).
    pub fn error_rate(&self, provider_id: &str) -> f64 {
        let g = self.inner.lock().unwrap();
        let runs = g.runs_started.get(provider_id).copied().unwrap_or(0);
        let errors: u64 = g
            .errors
            .get(provider_id)
            .map(|m| m.values().sum())
            .unwrap_or(0);
        let total = runs + errors;
        if total == 0 {
            0.0
        } else {
            errors as f64 / total as f64
        }
    }
}
