//! Tests for crash recovery and run reconciliation.

use chrono::{Duration, Utc};
use session_store::{
    ReconcileOutcome, ReconciledStatus, RecoveryPolicy, RecoveryRunRecord, RecoveryScanner,
    ResumeRecommendation, SafeResumePolicy, run_startup_recovery,
};

fn recent_record(run_id: &str, status: &str, had_tool_activity: bool) -> RecoveryRunRecord {
    RecoveryRunRecord {
        run_id: run_id.to_string(),
        session_id: Some("sess-1".to_string()),
        status: status.to_string(),
        created_at: Utc::now() - Duration::minutes(5),
        had_tool_activity,
    }
}

fn old_record(run_id: &str, status: &str) -> RecoveryRunRecord {
    RecoveryRunRecord {
        run_id: run_id.to_string(),
        session_id: Some("sess-old".to_string()),
        status: status.to_string(),
        // 25 hours ago — beyond the default 24h max_resumable_age
        created_at: Utc::now() - Duration::hours(25),
        had_tool_activity: false,
    }
}

// ---------------------------------------------------------------------------
// 1. Terminal runs are skipped
// ---------------------------------------------------------------------------
#[test]
fn test_terminal_runs_skipped() {
    let scanner = RecoveryScanner::with_default_policy();
    let runs = vec![
        recent_record("r1", "completed", false),
        recent_record("r2", "failed", false),
        recent_record("r3", "cancelled", false),
    ];
    let outcomes = scanner.scan_runs(&runs);
    assert!(outcomes.is_empty(), "terminal runs should produce no outcomes");
}

// ---------------------------------------------------------------------------
// 2. Recent conversational run → Resumable
// ---------------------------------------------------------------------------
#[test]
fn test_recent_conversational_run_is_resumable() {
    let scanner = RecoveryScanner::with_default_policy();
    let runs = vec![recent_record("r1", "running", false)];
    let outcomes = scanner.scan_runs(&runs);
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].reconciled_status, ReconciledStatus::Resumable);
}

// ---------------------------------------------------------------------------
// 3. Run with tool activity → RequiresApproval
// ---------------------------------------------------------------------------
#[test]
fn test_run_with_tool_activity_requires_approval() {
    let scanner = RecoveryScanner::with_default_policy();
    let runs = vec![recent_record("r1", "running", true)];
    let outcomes = scanner.scan_runs(&runs);
    assert_eq!(outcomes.len(), 1);
    assert_eq!(
        outcomes[0].reconciled_status,
        ReconciledStatus::RequiresApproval
    );
}

// ---------------------------------------------------------------------------
// 4. Run older than max_resumable_age → Cancelled
// ---------------------------------------------------------------------------
#[test]
fn test_old_run_is_cancelled() {
    let scanner = RecoveryScanner::with_default_policy();
    let runs = vec![old_record("r1", "running")];
    let outcomes = scanner.scan_runs(&runs);
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].reconciled_status, ReconciledStatus::Cancelled);
}

// ---------------------------------------------------------------------------
// 5. SafeResumePolicy: Resumable → AutoResume
// ---------------------------------------------------------------------------
#[test]
fn test_safe_resume_policy_auto_resume() {
    let outcome = make_outcome(ReconciledStatus::Resumable);
    assert_eq!(
        SafeResumePolicy::recommendation(&outcome),
        ResumeRecommendation::AutoResume
    );
}

// ---------------------------------------------------------------------------
// 6. SafeResumePolicy: RequiresApproval → AwaitApproval
// ---------------------------------------------------------------------------
#[test]
fn test_safe_resume_policy_await_approval() {
    let outcome = make_outcome(ReconciledStatus::RequiresApproval);
    assert_eq!(
        SafeResumePolicy::recommendation(&outcome),
        ResumeRecommendation::AwaitApproval
    );
}

// ---------------------------------------------------------------------------
// 7. SafeResumePolicy: Cancelled → DoNotResume
// ---------------------------------------------------------------------------
#[test]
fn test_safe_resume_policy_do_not_resume() {
    let outcome = make_outcome(ReconciledStatus::Cancelled);
    assert_eq!(
        SafeResumePolicy::recommendation(&outcome),
        ResumeRecommendation::DoNotResume
    );
}

// ---------------------------------------------------------------------------
// 8. run_startup_recovery with mixed runs returns correct outcomes
// ---------------------------------------------------------------------------
#[test]
fn test_startup_recovery_returns_outcomes() {
    let runs = vec![
        recent_record("r1", "completed", false), // terminal → skipped
        recent_record("r2", "running", false),   // resumable
        recent_record("r3", "running", true),    // requires approval
        old_record("r4", "running"),             // cancelled
    ];
    let outcomes = run_startup_recovery(runs);
    assert_eq!(outcomes.len(), 3);

    let statuses: Vec<&ReconciledStatus> = outcomes.iter().map(|o| &o.reconciled_status).collect();
    assert!(statuses.contains(&&ReconciledStatus::Resumable));
    assert!(statuses.contains(&&ReconciledStatus::RequiresApproval));
    assert!(statuses.contains(&&ReconciledStatus::Cancelled));
}

// ---------------------------------------------------------------------------
// 9. Empty run list → no outcomes
// ---------------------------------------------------------------------------
#[test]
fn test_empty_run_list_no_outcomes() {
    let outcomes = run_startup_recovery(vec![]);
    assert!(outcomes.is_empty());
}

// ---------------------------------------------------------------------------
// 10. ReconcileOutcome serializes to JSON cleanly
// ---------------------------------------------------------------------------
#[test]
fn test_reconcile_outcome_serializes() {
    let outcome = make_outcome(ReconciledStatus::Resumable);
    let json = serde_json::to_string(&outcome).expect("serialize");
    assert!(json.contains("\"resumable\""));
    assert!(json.contains("\"run_id\""));
    assert!(json.contains("\"reconciled_status\""));
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------
fn make_outcome(status: ReconciledStatus) -> ReconcileOutcome {
    ReconcileOutcome {
        run_id: "test-run".to_string(),
        session_id: Some("test-session".to_string()),
        original_status: "running".to_string(),
        reconciled_status: status,
        reason: "test".to_string(),
        timestamp: Utc::now(),
    }
}
