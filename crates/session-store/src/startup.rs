//! Startup recovery: scan for incomplete runs and reconcile them.

use crate::recovery::{ReconcileOutcome, RecoveryRunRecord, RecoveryScanner};
use tracing::{info, warn};

/// Run at startup. Scans provided run records and logs reconcile outcomes.
/// Returns the outcomes so the caller can surface them in diagnostics/TUI.
pub fn run_startup_recovery(runs: Vec<RecoveryRunRecord>) -> Vec<ReconcileOutcome> {
    let scanner = RecoveryScanner::with_default_policy();
    let outcomes = scanner.scan_runs(&runs);

    if outcomes.is_empty() {
        info!("startup recovery: no incomplete runs found");
    } else {
        warn!(
            "startup recovery: found {} incomplete run(s)",
            outcomes.len()
        );
        for outcome in &outcomes {
            warn!(
                "  run={} status={:?} reason={}",
                outcome.run_id, outcome.reconciled_status, outcome.reason
            );
        }
    }

    outcomes
}
