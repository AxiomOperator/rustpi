pub mod collector;
pub mod error;
pub mod provider;
pub mod summary;
pub mod tokens;
pub mod tools;

pub use collector::TelemetryCollector;
pub use error::ObservabilityError;
pub use provider::{ProviderMetrics, ProviderMetricsInner};
pub use summary::TelemetrySummary;
pub use tokens::{TokenUsageRecord, TokenUsageTracker};
pub use tools::{ToolFailureRecord, ToolMetrics, ToolMetricsInner};

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::{
        bus::EventBus,
        types::{AgentEvent, ModelId, ProviderId, RunId, SessionId},
    };
    use chrono::Utc;

    fn make_run_started(provider: &str) -> AgentEvent {
        AgentEvent::RunStarted {
            run_id: RunId::new(),
            session_id: SessionId::new(),
            provider: ProviderId::new(provider),
            model: ModelId::new("test-model"),
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn apply_run_started_increments_counter() {
        let c = TelemetryCollector::new();
        c.apply(&make_run_started("openai"));
        c.apply(&make_run_started("openai"));
        c.apply(&make_run_started("copilot"));
        let snap = c.provider.snapshot();
        assert_eq!(snap.runs_started.get("openai"), Some(&2));
        assert_eq!(snap.runs_started.get("copilot"), Some(&1));
    }

    #[test]
    fn apply_run_failed_records_error() {
        let c = TelemetryCollector::new();
        c.apply(&AgentEvent::RunFailed {
            run_id: RunId::new(),
            reason: "timeout occurred".to_string(),
            timestamp: Utc::now(),
        });
        let snap = c.provider.snapshot();
        let errs = snap.errors.get("unknown").unwrap();
        assert_eq!(errs.get("timeout"), Some(&1));
    }

    #[test]
    fn apply_token_chunk_accumulates() {
        let c = TelemetryCollector::new();
        let run_id = RunId::new();
        c.apply(&AgentEvent::TokenChunk {
            run_id: run_id.clone(),
            delta: "hello".to_string(),
            timestamp: Utc::now(),
        });
        c.apply(&AgentEvent::TokenChunk {
            run_id: run_id.clone(),
            delta: " world".to_string(),
            timestamp: Utc::now(),
        });
        assert_eq!(c.tokens.total_estimated_tokens(), 11);
        let rec = c.tokens.for_run(&run_id.to_string()).unwrap();
        assert_eq!(rec.estimated_output_tokens, 11);
    }

    #[test]
    fn apply_tool_failed_records_failure() {
        let c = TelemetryCollector::new();
        c.apply(&AgentEvent::ToolFailed {
            run_id: RunId::new(),
            call_id: "c1".to_string(),
            tool_name: "bash".to_string(),
            reason: "exit 1".to_string(),
            timestamp: Utc::now(),
        });
        assert_eq!(c.tools.total_failures(), 1);
        let snap = c.tools.snapshot();
        assert_eq!(snap.failures.get("bash"), Some(&1));
    }

    #[test]
    fn apply_tool_cancelled_increments_count() {
        let c = TelemetryCollector::new();
        c.apply(&AgentEvent::ToolCancelled {
            run_id: RunId::new(),
            call_id: "c1".to_string(),
            tool_name: "bash".to_string(),
            timestamp: Utc::now(),
        });
        c.apply(&AgentEvent::ToolExecutionCancelled {
            run_id: RunId::new(),
            call_id: "c2".to_string(),
            timestamp: Utc::now(),
        });
        let snap = c.tools.snapshot();
        assert_eq!(snap.cancellations, 2);
    }

    #[test]
    fn apply_run_cancelled_increments_cancellations() {
        let c = TelemetryCollector::new();
        c.apply(&AgentEvent::RunCancelled {
            run_id: RunId::new(),
            timestamp: Utc::now(),
        });
        let snap = c.provider.snapshot();
        assert_eq!(snap.cancellations, 1);
    }

    #[test]
    fn error_rate_calculation() {
        let m = ProviderMetrics::default();
        m.record_run_start("openai");
        m.record_run_start("openai");
        m.record_run_start("openai");
        m.record_error("openai", "timeout");
        // 1 error / (3 runs + 1 error) = 0.25
        let rate = m.error_rate("openai");
        assert!((rate - 0.25).abs() < 1e-9);
    }

    #[test]
    fn avg_latency_calculation() {
        let m = ProviderMetrics::default();
        m.record_latency("openai", 100);
        m.record_latency("openai", 200);
        m.record_latency("openai", 300);
        let avg = m.avg_latency_ms("openai").unwrap();
        assert!((avg - 200.0).abs() < 1e-9);
        assert!(m.avg_latency_ms("other").is_none());
    }

    #[test]
    fn token_total_accumulates_across_runs() {
        let t = TokenUsageTracker::default();
        t.record_chunk("run1", 10);
        t.record_chunk("run2", 20);
        t.record_chunk("run1", 5);
        assert_eq!(t.total_estimated_tokens(), 35);
        assert_eq!(t.for_run("run1").unwrap().estimated_output_tokens, 15);
        assert_eq!(t.for_run("run2").unwrap().estimated_output_tokens, 20);
    }

    #[test]
    fn snapshot_serializes_to_json() {
        let c = TelemetryCollector::new();
        c.apply(&make_run_started("openai"));
        let summary = c.snapshot();
        let json = serde_json::to_string(&summary);
        assert!(json.is_ok(), "serialization failed: {:?}", json.err());
    }

    #[tokio::test]
    async fn subscribe_to_bus_processes_events() {
        let c = TelemetryCollector::new();
        let bus = EventBus::new();
        let handle = c.subscribe_to_bus(&bus);

        bus.emit(make_run_started("copilot"));
        bus.emit(AgentEvent::RunCancelled {
            run_id: RunId::new(),
            timestamp: Utc::now(),
        });

        // Give the background task time to process events.
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        drop(bus); // closes the broadcast channel

        let _ = tokio::time::timeout(tokio::time::Duration::from_millis(200), handle).await;

        let snap = c.provider.snapshot();
        assert_eq!(snap.runs_started.get("copilot"), Some(&1));
        assert_eq!(snap.cancellations, 1);
    }

    // -----------------------------------------------------------------------
    // Phase 12 chaos / failure tests
    // -----------------------------------------------------------------------

    // -- Provider disconnect simulation --

    #[test]
    fn chaos_provider_consecutive_errors_no_panic() {
        // Feed 50 consecutive RunFailed events; must not panic and must accumulate correctly.
        let c = TelemetryCollector::new();
        for _ in 0..50 {
            c.apply(&AgentEvent::RunFailed {
                run_id: RunId::new(),
                reason: "connect: connection refused".to_string(),
                timestamp: Utc::now(),
            });
        }
        let snap = c.provider.snapshot();
        let network_errors: u64 = snap
            .errors
            .get("unknown")
            .and_then(|m| m.get("network"))
            .copied()
            .unwrap_or(0);
        assert_eq!(network_errors, 50, "all 50 errors should be recorded as network");
    }

    #[test]
    fn chaos_provider_error_rate_all_failures() {
        // When every request is a failure (0 successful runs started), error_rate should be 1.0.
        let m = ProviderMetrics::default();
        for _ in 0..10 {
            m.record_error("openai", "network");
        }
        // 10 errors, 0 runs → total = 10, rate = 10/10 = 1.0
        let rate = m.error_rate("openai");
        assert!(
            (rate - 1.0).abs() < 1e-9,
            "expected error rate 1.0, got {rate}"
        );
    }

    #[test]
    fn chaos_provider_error_rate_majority_failures() {
        // 1 success, 9 failures → error rate = 9/10 = 0.9
        let m = ProviderMetrics::default();
        m.record_run_start("anthropic");
        for _ in 0..9 {
            m.record_error("anthropic", "timeout");
        }
        let rate = m.error_rate("anthropic");
        assert!(
            (rate - 0.9).abs() < 1e-9,
            "expected error rate 0.9, got {rate}"
        );
    }

    #[test]
    fn chaos_provider_all_failed_summary_valid_json() {
        // Build a summary where every run has failed; JSON serialization must succeed.
        let c = TelemetryCollector::new();
        for _ in 0..5 {
            c.apply(&AgentEvent::RunFailed {
                run_id: RunId::new(),
                reason: "auth: 401 Unauthorized".to_string(),
                timestamp: Utc::now(),
            });
        }
        let summary = c.snapshot();
        let json = serde_json::to_string(&summary);
        assert!(json.is_ok(), "summary with all failures must serialize to JSON: {:?}", json.err());
        // The JSON must be parseable back into a Value.
        let v: serde_json::Value = serde_json::from_str(&json.unwrap()).unwrap();
        assert!(v.is_object(), "serialized summary should be a JSON object");
    }

    #[test]
    fn chaos_provider_auth_errors_classified_correctly() {
        // "auth: 401 Unauthorized" should be classified as auth_failure.
        let c = TelemetryCollector::new();
        c.apply(&AgentEvent::AuthLoginFailed {
            provider: ProviderId::new("github"),
            flow: agent_core::types::AuthFlow::OAuthBrowser,
            reason: "401 Unauthorized".to_string(),
            timestamp: Utc::now(),
        });
        let snap = c.provider.snapshot();
        let auth_errors = snap
            .errors
            .get("github")
            .and_then(|m| m.get("auth_failure"))
            .copied()
            .unwrap_or(0);
        assert_eq!(auth_errors, 1);
    }

    // -- Hung tool / timeout simulation --

    #[test]
    fn chaos_tool_record_cancellation() {
        let m = ToolMetrics::default();
        m.record_cancellation();
        let snap = m.snapshot();
        assert_eq!(snap.cancellations, 1);
    }

    #[test]
    fn chaos_tool_multiple_successive_cancellations() {
        let c = TelemetryCollector::new();
        for _ in 0..20 {
            c.apply(&AgentEvent::ToolCancelled {
                run_id: RunId::new(),
                call_id: "c-1".to_string(),
                tool_name: "bash".to_string(),
                timestamp: Utc::now(),
            });
        }
        let snap = c.tools.snapshot();
        assert_eq!(snap.cancellations, 20, "all 20 cancellations should be counted");
    }

    #[test]
    fn chaos_tool_failures_and_cancellations_independent() {
        // Failures and cancellations must accumulate in separate counters.
        let c = TelemetryCollector::new();
        for i in 0..5 {
            c.apply(&AgentEvent::ToolFailed {
                run_id: RunId::new(),
                call_id: format!("f-{i}"),
                tool_name: "python".to_string(),
                reason: "exit 1".to_string(),
                timestamp: Utc::now(),
            });
        }
        for i in 0..3 {
            c.apply(&AgentEvent::ToolCancelled {
                run_id: RunId::new(),
                call_id: format!("c-{i}"),
                tool_name: "bash".to_string(),
                timestamp: Utc::now(),
            });
        }
        let snap = c.tools.snapshot();
        assert_eq!(snap.cancellations, 3, "cancellations should be 3");
        assert_eq!(c.tools.total_failures(), 5, "failures should be 5");
    }

    #[test]
    fn chaos_tool_rolling_failure_record_cap() {
        // ToolMetrics should cap the recent_failures queue at 50 and not panic.
        let m = ToolMetrics::default();
        for i in 0..200 {
            m.record_failure("bash", &format!("error {i}"));
        }
        let snap = m.snapshot();
        assert_eq!(
            snap.recent_failures.len(),
            50,
            "recent_failures should be capped at 50"
        );
        assert_eq!(m.total_failures(), 200, "total failure count should be 200");
    }
}
