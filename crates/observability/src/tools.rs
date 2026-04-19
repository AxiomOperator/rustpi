use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

const MAX_FAILURE_RECORDS: usize = 50;

#[derive(Debug, Default)]
pub struct ToolMetrics {
    inner: Mutex<ToolMetricsInner>,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct ToolMetricsInner {
    /// Failure count by tool_name.
    pub failures: HashMap<String, u64>,
    /// Cancellation count (total).
    pub cancellations: u64,
    /// Recent failure reasons (rolling last 50).
    pub recent_failures: VecDeque<ToolFailureRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolFailureRecord {
    pub tool_name: String,
    pub reason: String,
    pub timestamp: DateTime<Utc>,
}

impl ToolMetrics {
    pub fn record_failure(&self, tool_name: &str, reason: &str) {
        let mut g = self.inner.lock().unwrap();
        *g.failures.entry(tool_name.to_string()).or_insert(0) += 1;
        if g.recent_failures.len() >= MAX_FAILURE_RECORDS {
            g.recent_failures.pop_front();
        }
        g.recent_failures.push_back(ToolFailureRecord {
            tool_name: tool_name.to_string(),
            reason: reason.to_string(),
            timestamp: Utc::now(),
        });
    }

    pub fn record_cancellation(&self) {
        let mut g = self.inner.lock().unwrap();
        g.cancellations += 1;
    }

    pub fn snapshot(&self) -> ToolMetricsInner {
        self.inner.lock().unwrap().clone()
    }

    pub fn total_failures(&self) -> u64 {
        self.inner.lock().unwrap().failures.values().sum()
    }
}
