Complete **Phase 12 — Observability, replay, and reliability hardening** for this project.

## Goal

Make the system debuggable, recoverable, and durable by adding replay tooling, diagnostics surfaces, provider/tool/runtime telemetry, token usage tracking, crash recovery, and safe resume behavior with meaningful failure-path testing.

## Critical instructions

- Fully inspect the current codebase and architecture before making changes.
- Preserve sound architectural patterns already present, but refactor where needed to support a clean long-term observability and reliability layer.
- Build this as real production-oriented infrastructure, not as a thin debug-only add-on.
- Do not leave fake telemetry, placeholder replay features, or partial crash recovery logic presented as complete.
- If prerequisite foundations from earlier phases are incomplete, implement the minimum required support so this phase is correct and integrated.
- Keep replay, diagnostics, metrics/telemetry, recovery logic, reconciliation logic, and failure testing clearly separated behind clean interfaces.
- Ensure the resulting system is meaningfully diagnosable and auditable for operators.
- Ensure you update both `README.md` and `project.md` to reflect architecture, commands, metrics, recovery behavior, limitations, and phase progress.
- When finished, mark completed items in `project.md` if that file tracks progress.
- Output a concise completion summary, the major files changed, key design decisions, and any deferred follow-up work.

---

# Phase 12 — Observability, replay, and reliability hardening

## Objectives

Make the system debuggable, recoverable, and durable.

## Deliverables

- [ ] replay tooling
- [ ] session diagnostics
- [ ] provider latency/error tracking
- [ ] token usage tracking
- [ ] crash recovery behavior
- [ ] safe resume behavior

## Tasks

- [ ] Build session replay viewer
- [ ] Build diagnostics commands
- [ ] Track:
  - [ ] provider latency
  - [ ] provider error rate
  - [ ] token usage
  - [ ] tool failures
  - [ ] cancellation counts
- [ ] Implement resume-after-crash logic
- [ ] Implement incomplete-run reconciliation
- [ ] Add chaos/failure tests for:
  - [ ] provider disconnects
  - [ ] hung tool execution
  - [ ] partial event log writes
  - [ ] token refresh failure

## Exit criteria

- [ ] Failures are diagnosable
- [ ] Sessions can recover cleanly after interruption
- [ ] Runtime behavior is observable and auditable

---

## Required implementation approach

### 1. Audit the existing architecture first

Before writing code:

- Review the current runtime/session architecture, durable stores, event logs, tool runtime, provider adapters, auth/token refresh handling, CLI/TUI/RPC surfaces, diagnostics hooks, and test structure.
- Identify what observability already exists, if any, and where metrics/events/errors are currently emitted or lost.
- Determine the correct architectural boundaries for:
  - replay
  - diagnostics
  - telemetry/metrics
  - crash recovery
  - incomplete-run reconciliation
- Refactor fragmented logging or ad hoc failure handling if needed so this phase results in one coherent observability and reliability layer.

### 2. Define a clear observability architecture

Implement or refine a structured observability/reliability design with clearly separated layers for:

- event capture/persistence
- replay/read models
- diagnostics queries/commands
- metrics and counters
- provider/tool/runtime telemetry
- token usage accounting
- crash recovery bootstrap
- incomplete-run reconciliation
- safe resume policy
- failure/chaos test harnesses

Do not bolt these on as scattered one-off debug helpers.

### 3. Build session replay viewer

Implement a real session replay viewer.

Requirements:
- reconstruct a session/run timeline from persisted state and event history
- render ordered events in a stable, operator-usable form
- support replaying conversation/tool/runtime activity with enough fidelity to diagnose what happened
- handle missing/partial records gracefully
- work from durable persisted data rather than ephemeral in-memory state
- integrate with whichever user-facing surfaces are appropriate in the current project architecture
  - CLI command, TUI pane/view, internal viewer, or similar

At minimum, the replay viewer should let an operator answer:
- what happened
- in what order
- which provider/tool was involved
- where failure/cancellation/resume occurred

### 4. Build diagnostics commands

Implement diagnostics commands/surfaces.

Requirements:
- expose useful runtime and backend health/diagnostic information
- integrate with existing CLI and/or RPC surfaces as appropriate
- return machine-usable data where suitable
- include state relevant to:
  - sessions
  - runs
  - backends
  - providers
  - auth/token state
  - tool runtime
  - recent failures
- keep diagnostics structured and actually useful for operators

Do not settle for generic “it works / it failed” output.

### 5. Track provider latency

Implement provider latency tracking.

Requirements:
- capture latency for provider interactions in a structured way
- support at least per-provider and preferably per-operation visibility where appropriate
- store or expose telemetry so operators can inspect slow providers
- keep the implementation backend-agnostic from the runtime’s perspective
- ensure latency tracking works for both successful and failed requests where practical

### 6. Track provider error rate

Implement provider error tracking.

Requirements:
- capture provider failures in a normalized way
- support error-rate visibility by provider and possibly by operation/category
- distinguish useful categories such as auth failure, rate limiting, timeout, upstream failure, malformed response, etc.
- ensure this integrates with existing provider error normalization from earlier phases

This should help diagnose provider reliability, not just count generic failures.

### 7. Track token usage

Implement token usage tracking.

Requirements:
- track token usage for model interactions where available
- normalize token accounting across providers as much as practical
- handle providers that do not report token usage directly with a documented fallback strategy if necessary
- expose token usage in diagnostics and/or persisted telemetry
- keep usage associated with relevant session/run/provider/model context where appropriate

Avoid fake precision. If token usage is estimated for some providers, mark it clearly.

### 8. Track tool failures and cancellation counts

Implement tracking for:
- tool failures
- cancellation counts

Requirements:
- count and classify tool failures meaningfully
- count cancellations distinctly from failures
- preserve enough metadata to diagnose what failed and under what circumstances
- integrate this into diagnostics/replay/telemetry surfaces

### 9. Implement resume-after-crash logic

Implement real resume-after-crash behavior.

Requirements:
- on startup, detect interrupted/incomplete runtime state
- reload durable sessions/runs/events as needed
- determine whether interrupted runs are resumable, non-resumable, or require reconciliation
- preserve data integrity and auditability
- avoid accidentally claiming success for incomplete work
- avoid blindly restarting dangerous operations without policy

Resume-after-crash must be deliberate and safe, not automatic chaos.

### 10. Implement incomplete-run reconciliation

Implement incomplete-run reconciliation logic.

Requirements:
- detect runs left in an indeterminate state after crash/interruption
- reconcile them into explicit post-crash states such as:
  - resumable
  - cancelled
  - failed
  - requires operator review
  - completed but finalization missing
- define deterministic reconciliation rules
- persist the reconciled result
- surface reconciliation outcomes in diagnostics/replay

This is especially important for interrupted provider calls, partially completed tool executions, and partially written event logs.

### 11. Implement safe resume behavior

Safe resume behavior must be explicit and conservative.

Requirements:
- define what kinds of runs/actions are safe to resume automatically vs only by approval vs not resumable
- ensure destructive/high-impact operations are not silently resumed
- preserve context needed for operator decision-making
- surface resume safety/status to the operator through diagnostics/replay and/or runtime surfaces
- document the safe resume policy clearly

A sensible default:
- read-only or conversational runs may be resumable
- tool actions with side effects often require operator review/approval
- incomplete provider/tool outputs should be clearly marked, not treated as finished

### 12. Handle partial event log writes and imperfect data

The system must remain diagnosable even when data is imperfect.

Requirements:
- tolerate partial or truncated event log writes where practical
- detect corruption/incomplete records clearly
- preserve usable parts of the event history where possible
- avoid crashing diagnostics/replay because one record is malformed
- document data-recovery/fallback behavior

### 13. Add chaos and failure-path tests

Add meaningful chaos/failure tests for:

#### Required
- provider disconnects
- hung tool execution
- partial event log writes
- token refresh failure

#### Also add where appropriate
- process interruption during run finalization
- provider timeout during streaming
- backend unavailable during replay/diagnostics
- resume of runs with mixed complete/incomplete artifacts
- cancellation during tool output streaming
- corrupted persisted state edge cases

Tests should validate the real recovery and observability paths, not only happy-path abstractions.

### 14. Error handling and normalization

Ensure observability and recovery code has strong error handling.

Requirements:
- distinguish telemetry/reporting failures from core runtime failures
- preserve useful debug details for operators
- do not let replay/diagnostics failure crash the primary runtime unnecessarily
- normalize recovery/reconciliation errors into clear operator-facing states
- avoid silent dropping of important failure information

### 15. Make the system auditable

Where appropriate, ensure persisted telemetry and replay artifacts are useful for audit.

Requirements:
- maintain timestamps and IDs consistently
- preserve ordering metadata where possible
- associate metrics/events with session/run/provider/tool context
- ensure operators can determine who/what/when for major actions and failures
- avoid opaque internal-only states that cannot be explained later

### 16. Keep observability separate from core runtime logic

The observability layer should integrate deeply, but remain architecturally clean.

Requirements:
- keep metric collection and diagnostics interfaces separate from core execution logic where possible
- avoid scattering counters/timers/error trackers randomly across the codebase
- centralize telemetry emission patterns
- make replay/read-model logic separate from raw write-time event emission when appropriate

### 17. Documentation updates

You must update:

#### `project.md`
Include:
- phase progress updates
- completed deliverables/tasks checkboxes
- architecture notes for observability and reliability hardening
- replay/diagnostics notes
- crash recovery and reconciliation notes
- tracked telemetry/metrics notes
- known limitations or deferred follow-up work

#### `README.md`
Include:
- overview of replay tooling
- diagnostics commands or usage
- what metrics/telemetry are tracked
- token usage tracking behavior
- provider/tool reliability telemetry behavior
- crash recovery behavior
- incomplete-run reconciliation behavior
- safe resume behavior
- testing instructions for failure/chaos cases
- caveats/limitations

If helpful, add concise sections/tables showing:
- metric/telemetry item
- where it is collected
- where it is exposed
- whether it is exact or estimated

---

## Implementation quality bar

- Build real operator-usable replay and diagnostics.
- Make failures explainable after the fact.
- Make recovery deliberate and safe.
- Make telemetry structured and useful.
- Preserve auditability.
- Avoid pretending uncertain token usage is exact.
- Avoid brittle replay that breaks on imperfect data.
- Avoid silently resuming unsafe work.

## Deliverable expectations

When complete, provide:

1. A summary of what was implemented
2. The main files created or updated
3. The key architectural decisions
4. Any limitations or deferred follow-up work
5. Confirmation that `README.md` and `project.md` were updated

Do not stop at scaffolding. Complete the phase to a working standard consistent with the project’s architecture.