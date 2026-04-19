//! Crash recovery and run reconciliation.
//!
//! On startup, the RecoveryScanner examines stored runs for incomplete states.
//! It classifies them and either auto-recovers them or marks them for operator review.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Policy controlling what happens to incomplete runs after crash.
#[derive(Debug, Clone)]
pub struct RecoveryPolicy {
    /// Maximum age of a run before it is considered unresumable (default: 24h).
    pub max_resumable_age: std::time::Duration,
    /// Whether read-only/conversational runs can be auto-resumed.
    pub auto_resume_conversational: bool,
    /// Whether runs with tool side-effects require operator approval before resume.
    pub require_approval_for_tool_runs: bool,
}

impl Default for RecoveryPolicy {
    fn default() -> Self {
        Self {
            max_resumable_age: std::time::Duration::from_secs(24 * 3600),
            auto_resume_conversational: true,
            require_approval_for_tool_runs: true,
        }
    }
}

/// The outcome of reconciling a single incomplete run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconcileOutcome {
    pub run_id: String,
    pub session_id: Option<String>,
    pub original_status: String,
    pub reconciled_status: ReconciledStatus,
    pub reason: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReconciledStatus {
    /// Safe to auto-resume (conversational, recent, no side effects).
    Resumable,
    /// Requires operator approval before resuming (had tool side effects).
    RequiresApproval,
    /// Too old or too far gone — marked as cancelled.
    Cancelled,
    /// Had a partial failure — marked as failed.
    Failed,
    /// Completed during reconciliation check.
    AlreadyComplete,
}

/// Scanner that runs at startup to detect and reconcile incomplete runs.
pub struct RecoveryScanner {
    policy: RecoveryPolicy,
}

impl RecoveryScanner {
    pub fn new(policy: RecoveryPolicy) -> Self {
        Self { policy }
    }

    pub fn with_default_policy() -> Self {
        Self::new(RecoveryPolicy::default())
    }

    /// Scan a list of run records for incomplete states.
    /// Returns reconcile outcomes for any that need attention.
    pub fn scan_runs(&self, runs: &[RecoveryRunRecord]) -> Vec<ReconcileOutcome> {
        let now = Utc::now();
        let mut outcomes = Vec::new();

        for run in runs {
            if is_terminal(&run.status) {
                continue; // already complete
            }

            let age = now.signed_duration_since(run.created_at);
            let age_std = age.to_std().unwrap_or(std::time::Duration::from_secs(0));

            let reconciled_status = if age_std > self.policy.max_resumable_age {
                ReconciledStatus::Cancelled
            } else if run.had_tool_activity && self.policy.require_approval_for_tool_runs {
                ReconciledStatus::RequiresApproval
            } else if self.policy.auto_resume_conversational {
                ReconciledStatus::Resumable
            } else {
                ReconciledStatus::RequiresApproval
            };

            outcomes.push(ReconcileOutcome {
                run_id: run.run_id.clone(),
                session_id: run.session_id.clone(),
                original_status: run.status.clone(),
                reconciled_status,
                reason: format!(
                    "run was in status '{}' at startup (age: {}s)",
                    run.status,
                    age_std.as_secs()
                ),
                timestamp: now,
            });
        }
        outcomes
    }
}

/// Lightweight run record used for recovery scanning (doesn't require full store).
#[derive(Debug, Clone)]
pub struct RecoveryRunRecord {
    pub run_id: String,
    pub session_id: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    /// Whether this run included any tool execution events.
    pub had_tool_activity: bool,
}

fn is_terminal(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "cancelled")
}

// ---------------------------------------------------------------------------
// SafeResumePolicy
// ---------------------------------------------------------------------------

/// Determines whether a run is safe to resume automatically.
pub struct SafeResumePolicy;

impl SafeResumePolicy {
    /// Returns the resume recommendation for a run given its reconcile outcome.
    pub fn recommendation(outcome: &ReconcileOutcome) -> ResumeRecommendation {
        match outcome.reconciled_status {
            ReconciledStatus::Resumable => ResumeRecommendation::AutoResume,
            ReconciledStatus::RequiresApproval => ResumeRecommendation::AwaitApproval,
            ReconciledStatus::Cancelled | ReconciledStatus::Failed => {
                ResumeRecommendation::DoNotResume
            }
            ReconciledStatus::AlreadyComplete => ResumeRecommendation::DoNotResume,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResumeRecommendation {
    AutoResume,
    AwaitApproval,
    DoNotResume,
}
