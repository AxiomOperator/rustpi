Complete **Phase 14 — Full-system testing and parity validation** for this project.

## Goal

Validate that the platform actually achieves the intended feature set by building comprehensive unit and integration coverage, running backend/provider/interface/failure-mode validation, and conducting a real feature parity review against the architecture and declared phases.

## Critical instructions

- Fully inspect the current codebase, architecture, and all prior phase deliverables before making changes.
- Preserve sound architectural patterns already present, but refactor tests, harnesses, and validation tooling where needed to support a clean long-term validation strategy.
- Build this as a real release-readiness validation pass, not as a cosmetic test-count exercise.
- Do not leave fake coverage claims, placeholder matrix tests, or unchecked parity assertions presented as complete.
- If prerequisite foundations from earlier phases are incomplete, identify and complete the minimum required work so that Phase 14 validates a real, coherent system rather than a partial scaffold.
- Keep unit testing, integration testing, matrix testing, failure-mode validation, parity review, and release-readiness reporting clearly separated but coordinated.
- Ensure you update both `README.md` and `project.md` to reflect the test architecture, validation coverage, known gaps, release-readiness status, and phase progress.
- When finished, mark completed items in `project.md` if that file tracks progress.
- Output a concise completion summary, the major files changed, key design decisions, any uncovered gaps, and any deferred follow-up work.

---

# Phase 14 — Full-system testing and parity validation

## Objectives

Validate that the platform achieves the intended feature set.

## Deliverables

- [ ] unit test coverage across crates
- [ ] integration test suite
- [ ] backend matrix tests
- [ ] provider matrix tests
- [ ] CLI/TUI test checklist
- [ ] memory sync test checklist
- [ ] release-readiness checklist

## Tasks

- [ ] Unit tests for each crate
- [ ] Integration tests for full run lifecycle
- [ ] Provider tests:
  - [ ] OpenAI-compatible
  - [ ] llama.cpp
  - [ ] vLLM
  - [ ] first OAuth/device-auth provider
- [ ] Storage tests:
  - [ ] SQLite
  - [ ] `sled`
  - [ ] PostgreSQL
  - [ ] Qdrant
  - [ ] Obsidian vault sync
- [ ] Interface tests:
  - [ ] RPC
  - [ ] CLI
  - [ ] TUI
- [ ] Failure mode tests:
  - [ ] token expiry
  - [ ] network drop
  - [ ] hung tool
  - [ ] corrupted note
  - [ ] partial replay log
- [ ] Run feature parity review against architecture

## Exit criteria

- [ ] All declared core features are implemented
- [ ] Core backends and provider paths are tested
- [ ] The system is stable enough for internal daily use

---

## Required implementation approach

### 1. Audit the entire platform first

Before writing code:

- Review all crates/modules, prior phase deliverables, current architecture docs, current test coverage, existing harnesses, CI configuration, known TODOs, and any incomplete or partially implemented features.
- Build a concrete map of:
  - crates/modules
  - runtime flows
  - provider adapters
  - storage backends
  - interfaces (RPC/CLI/TUI)
  - memory and sync systems
  - recovery/security/observability surfaces
- Identify missing test coverage, missing harnesses, and any claimed features that are not actually fully implemented.
- Do not assume earlier phases are complete just because the docs say so. Verify against code.

### 2. Define a full validation strategy

Implement or refine a validation/testing strategy with clearly separated layers for:

- crate-level unit tests
- cross-crate integration tests
- backend matrix tests
- provider matrix tests
- interface tests
- failure-mode tests
- parity review/reporting
- release-readiness checklisting

The validation strategy should reflect the actual system architecture, not just a pile of isolated tests.

### 3. Unit tests for each crate

Add meaningful unit tests for every crate/module in the project.

Requirements:
- cover core logic in each crate, not just trivial getters/setters
- prioritize domain logic, normalization logic, state transitions, validation, parsing, policy checks, and error handling
- ensure crate-level tests are maintainable and not overly coupled to unrelated integration details
- identify any crates with weak or missing testability and refactor as needed to enable proper coverage

Do not pad the suite with low-value tests to inflate counts.

### 4. Integration tests for the full run lifecycle

Build integration tests that validate the real end-to-end run lifecycle.

At minimum, cover:
- session creation/selection
- prompt/task submission
- context assembly
- provider execution
- streaming behavior
- tool runtime participation where relevant
- persistence/event recording
- completion/failure/cancellation flows
- replay/diagnostics visibility where appropriate

Requirements:
- use the actual runtime boundaries as much as practical
- avoid bypassing important orchestration layers
- ensure the integration suite validates a coherent system, not just adjacent helpers

### 5. Provider matrix tests

Add provider tests for:

- OpenAI-compatible
- llama.cpp
- vLLM
- first OAuth/device-auth provider

Requirements:
- validate chat/completion behavior as applicable
- validate streaming behavior
- validate model discovery behavior
- validate auth/status behavior where relevant
- validate normalized error handling
- validate provider capability declarations
- use mocks/harnesses where live provider dependency would make tests brittle
- if live tests exist, keep them optional and clearly separated from required CI-safe tests

The matrix should verify that providers actually conform to the shared internal contract.

### 6. Storage/backend matrix tests

Add storage tests for:

- SQLite
- `sled`
- PostgreSQL
- Qdrant
- Obsidian vault sync

Requirements:
- validate behavior through shared store/memory abstractions where practical
- verify backend parity, not just backend-specific happy paths
- verify restart recovery and persistence semantics where relevant
- verify migration/version safety where relevant
- verify vault sync and retrieval index behavior
- verify structured store ↔ vault sync behavior where relevant

Do not let backend-specific behavior silently become the platform contract unless explicitly documented.

### 7. Interface tests

Add interface tests for:

- RPC
- CLI
- TUI

Requirements:
- RPC: validate protocol framing, request/response handling, event streaming, session attach/detach, run control, and error normalization
- CLI: validate prompt submission, JSON mode, print mode, piped I/O, file execution mode, session resume/select, provider/model flags, auth/diagnostics commands
- TUI: add meaningful rendering/interaction/snapshot/state-flow tests where practical, especially for approval/interrupt flows, streaming rendering, session navigation, and provider/auth views

The interface tests should verify that the public surfaces are actually usable and aligned with the runtime.

### 8. Failure mode tests

Add meaningful failure-mode tests for:

- token expiry
- network drop
- hung tool
- corrupted note
- partial replay log

Requirements:
- token expiry: validate auth refresh/failure behavior and surfaced errors
- network drop: validate provider disconnect handling and recovery/failure semantics
- hung tool: validate timeout, cancellation, and event/reporting behavior
- corrupted note: validate vault parsing/sync/prompt-assembly resilience
- partial replay log: validate replay/diagnostics resilience to truncated/corrupted event history

Also add related failure tests where appropriate, such as:
- provider timeout during streaming
- backend unavailable at startup
- failed migration
- approval denial during destructive operation
- malformed RPC request
- invalid CLI flag combinations
- context packer fallback on summarization failure

These tests must exercise real failure-handling paths, not just mock return codes.

### 9. Build backend and provider matrix harnesses cleanly

Implement shared test harnesses/utilities for:
- provider contract testing
- backend contract testing
- session/run lifecycle setup
- vault sync setup
- RPC/CLI/TUI integration setup

Requirements:
- avoid copy-paste duplication across matrix tests
- keep harnesses explicit and debuggable
- make it easy to add future backends/providers without rewriting all tests
- ensure the matrix tests are stable and maintainable

### 10. Create CLI/TUI test checklist

Create a real CLI/TUI test checklist artifact.

Requirements:
- document manual and/or automated validation points for operator-facing flows
- include:
  - core interaction flows
  - streaming readability
  - auth flow handling
  - approval/interrupt workflows
  - session selection/resume
  - diagnostics/replay access
  - failure handling from operator surfaces
- place this checklist somewhere appropriate in the repo and reference it from `README.md` and/or `project.md`

This is not a substitute for automation; it complements it.

### 11. Create memory sync test checklist

Create a real memory sync test checklist artifact.

Requirements:
- cover structured store → vault sync
- cover vault → retrieval index sync
- cover malformed note handling
- cover duplicate note handling
- cover sync conflict handling
- cover prompt assembly from canonical docs
- cover read-only / approval-gated doc behavior if applicable

Place this checklist somewhere appropriate in the repo and reference it in docs.

### 12. Create a release-readiness checklist

Create a real release-readiness checklist.

Requirements:
- include core runtime readiness
- provider readiness
- backend readiness
- RPC/CLI/TUI readiness
- observability/replay readiness
- security hardening readiness
- memory/vault sync readiness
- recovery/resume readiness
- known limitations and manual validation gates

This should be a practical internal go/no-go artifact, not a generic template.

### 13. Run a feature parity review against the architecture

Perform a real feature parity review against the architecture and declared phases.

Requirements:
- compare the implemented system against the architecture and prior phase goals
- identify:
  - implemented and validated
  - implemented but weakly validated
  - partially implemented
  - missing
  - intentionally deferred
- update `project.md` with an honest status
- do not mark boxes complete unless the feature is actually implemented and validated to a reasonable standard

This is a truth-telling exercise, not a marketing pass.

### 14. Add coverage/reporting improvements where appropriate

Where practical:
- improve test organization
- improve CI/test commands
- add coverage collection/reporting if the project architecture supports it cleanly
- add grouped test entrypoints or make targets/scripts for the main validation suites
- document how to run:
  - unit tests
  - integration tests
  - matrix tests
  - failure tests
  - checklists/manual validation

Do not over-engineer coverage tooling if it adds little value, but do make the validation strategy runnable and clear.

### 15. Error handling and test failure quality

Requirements:
- failing tests should produce actionable diagnostics
- matrix test failures should identify which backend/provider/interface failed and why
- avoid opaque generic harness failures
- ensure parity review artifacts remain readable and honest even when failures exist

### 16. Documentation updates

You must update:

#### `project.md`
Include:
- phase progress updates
- completed deliverables/tasks checkboxes
- honest parity review results
- validation coverage summary
- known gaps or deferred work
- release-readiness notes

#### `README.md`
Include:
- overview of the test strategy
- how to run unit tests
- how to run integration tests
- how to run provider/backend matrix tests
- how to run interface tests
- how to run failure-mode tests
- where to find the CLI/TUI checklist
- where to find the memory sync checklist
- where to find the release-readiness checklist
- known limitations or incomplete parity areas

If helpful, add concise tables showing:
- test category
- scope
- command/path
- automated vs manual
- required vs optional

---

## Implementation quality bar

- Validate the real system, not just isolated pieces.
- Be honest about parity gaps.
- Build reusable matrix harnesses instead of copy-paste tests.
- Prioritize high-value behavior and failure-path coverage.
- Make provider/backend/interface behavior comparable and diagnosable.
- Keep manual checklists practical and specific.
- Avoid fake “done” status for unvalidated features.

## Deliverable expectations

When complete, provide:

1. A summary of what was implemented
2. The main files created or updated
3. The key architectural and validation decisions
4. Any uncovered gaps or deferred follow-up work
5. Confirmation that `README.md` and `project.md` were updated

Do not stop at scaffolding. Complete the phase to a working standard consistent with the project’s architecture, and be explicit anywhere parity is incomplete.