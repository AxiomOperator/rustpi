Complete **Phase 13 — Security hardening** for this project.

## Goal

Harden secrets handling, permissions, file mutation behavior, execution boundaries, and auditability so the system is safer to operate in real environments without leaking sensitive data or allowing overly broad destructive actions.

## Critical instructions

- Fully inspect the current codebase and architecture before making changes.
- Preserve sound architectural patterns already present, but refactor where needed to support a clean long-term security model.
- Build this as real security hardening, not as a cosmetic checklist pass.
- Do not leave fake encryption, placeholder redaction, weak path checks, or partial approval logic presented as complete.
- If prerequisite foundations from earlier phases are incomplete, implement the minimum required support so this phase is correct and integrated.
- Keep secrets handling, permission checks, tool execution policy, file mutation safeguards, audit logging, and redaction clearly separated behind clean interfaces.
- Treat this phase as high-sensitivity infrastructure work: prioritize safety, explicit behavior, and auditability over convenience.
- Ensure you update both `README.md` and `project.md` to reflect architecture, controls, security boundaries, limitations, and phase progress.
- When finished, mark completed items in `project.md` if that file tracks progress.
- Output a concise completion summary, the major files changed, key design decisions, and any deferred follow-up work.

---

# Phase 13 — Security hardening

## Objectives

Harden secrets, permissions, and execution boundaries.

## Deliverables

- [ ] secure token storage
- [ ] tool permission controls
- [ ] file mutation safeguards
- [ ] audit logging
- [ ] secrets redaction

## Tasks

- [ ] Encrypt persisted tokens
- [ ] Redact secrets from logs/events
- [ ] Restrict tool execution paths
- [ ] Add allow/deny command lists
- [ ] Add path traversal protections
- [ ] Add file overwrite safeguards
- [ ] Add approval requirements for destructive actions
- [ ] Conduct threat model review
- [ ] Add security-focused tests

## Exit criteria

- [ ] Tokens and secrets are handled safely
- [ ] Destructive actions are bounded and reviewable
- [ ] Logs and memory stores avoid secret leakage

---

## Required implementation approach

### 1. Audit the current security posture first

Before writing code:

- Review the current architecture for:
  - token/auth storage
  - provider auth flows
  - runtime memory stores
  - vault/Markdown memory
  - tool runtime and subprocess execution
  - file read/write/edit tools
  - diagnostics/logging/event emission
  - replay/session stores
  - config and environment-variable handling
- Identify where secrets may currently be stored, logged, indexed, replayed, or leaked.
- Identify current execution boundaries and permission checks, if any.
- Identify all places where destructive behavior can occur:
  - tool execution
  - file write/edit/delete/overwrite behavior
  - approval flows
  - persistence of sensitive material
- Refactor fragmented security logic if needed so this phase results in one coherent security model.

### 2. Define a clear security architecture

Implement or refine a security architecture with clearly separated layers for:

- token/credential storage
- encryption/key management abstraction
- redaction/sanitization
- tool permission policy
- command allow/deny policy
- path safety and traversal defense
- file mutation safety policy
- approval requirements for destructive actions
- audit logging
- security test harnesses

Do not scatter security checks ad hoc throughout unrelated codepaths.

### 3. Implement secure token storage

Implement secure persisted token storage.

Requirements:
- persisted tokens must be encrypted at rest
- encryption must be real and appropriate for the language/runtime and deployment model
- keep encryption/decryption behind a clean abstraction
- do not hardcode keys in source
- support a documented key-loading/configuration mechanism
- fail clearly and safely if encryption keys/configuration are missing or invalid
- avoid logging plaintext tokens during load/save/error paths

This applies to provider tokens, refresh tokens, device auth artifacts, and any similarly sensitive persisted auth material.

### 4. Define a key-management/config model

Implement or document a sane key-management approach appropriate for the project.

Requirements:
- define where encryption keys come from
- define startup validation behavior
- support a secure configuration path compatible with the existing config system
- avoid inventing a fake enterprise KMS if none exists, but still build a clean abstraction that could support one later
- document operational expectations in `README.md`

The MVP must still be meaningfully secure, even if advanced enterprise key rotation is deferred.

### 5. Redact secrets from logs and events

Implement robust secrets redaction for logs, events, diagnostics, replay surfaces, and persisted memory/event data.

Requirements:
- redact secrets before they are written to logs, events, diagnostics, replay artifacts, or memory stores
- cover likely secret forms such as:
  - access tokens
  - refresh tokens
  - API keys
  - bearer tokens
  - authorization headers
  - secret-looking config values
  - credentials embedded in URLs or command lines where feasible
- centralize redaction logic so all major output paths share the same protection
- preserve enough non-secret structure for debugging when possible
- ensure redaction happens before persistence where appropriate, not only at display time

Do not rely on operators to remember not to print secrets.

### 6. Restrict tool execution paths

Implement tool permission controls that bound where and how tools may operate.

Requirements:
- centralize tool execution policy
- restrict subprocess/file/search/edit tools to allowed execution and path boundaries
- enforce path-based safety checks for all filesystem-mutating actions
- ensure the runtime cannot bypass these checks through alternate codepaths
- keep policy inspectable and configurable enough for future refinement

This should integrate with the existing tool runtime from prior phases.

### 7. Add allow/deny command lists

Implement command allow/deny controls for subprocess execution.

Requirements:
- define how command policies are represented
- support explicit deny lists for dangerous commands/patterns
- support allow-listing where appropriate
- make policy evaluation happen before execution
- ensure denied commands fail clearly and auditably
- document limitations and matching behavior clearly

Do not rely only on string-equality checks if the system already supports richer command metadata or argument inspection.

### 8. Add path traversal protections

Implement strong path traversal protections.

Requirements:
- normalize and validate paths before use
- prevent traversal outside allowed roots
- apply protections consistently across:
  - read
  - write
  - edit
  - overwrite
  - any file-derived subprocess operations
- handle symlink edge cases appropriately for the project’s platform/runtime where practical
- fail clearly for unsafe paths

This must be centralized and testable, not duplicated inconsistently.

### 9. Add file overwrite safeguards

Implement file mutation and overwrite safeguards.

Requirements:
- prevent unsafe blind overwrites
- define when overwrite is allowed, denied, or approval-gated
- support safeguards for destructive edits or replacement operations
- preserve auditability of file mutations
- avoid silent destructive changes to protected or high-risk files
- integrate with approval workflows where necessary

A strong MVP should distinguish routine safe writes from destructive overwrites of important files.

### 10. Add approval requirements for destructive actions

Implement explicit approval requirements for destructive or high-risk actions.

Requirements:
- define what counts as destructive/high-risk
- ensure approval is enforced in runtime policy, not only in UI
- integrate with existing approval flows from prior phases
- ensure denied or missing approvals fail clearly and safely
- ensure approval events are auditable

Examples may include:
- dangerous shell commands
- broad file overwrites
- destructive edits
- operations outside normal safe roots
- sensitive identity/auth-related mutations if applicable

Do not silently downgrade destructive actions into unreviewed execution.

### 11. Protect logs, memory, and replay from secret leakage

Ensure that secrets do not leak into:
- logs
- event stores
- replay artifacts
- memory stores
- vault/Markdown memory
- diagnostics outputs

Requirements:
- review all persistence surfaces
- sanitize or block sensitive content before persistence where appropriate
- ensure replay/diagnostics viewers do not re-expose secrets from older records where protection can be applied
- document any unavoidable limitations clearly

### 12. Conduct a threat model review

Conduct and document a practical threat model review for the current architecture.

At minimum, address:
- secret theft from persisted storage
- secret leakage through logs/events/replay/memory
- unsafe subprocess execution
- path traversal / unsafe file mutation
- misuse of approval gaps
- host/operator misuse
- malformed external inputs through CLI/RPC/TUI surfaces
- unsafe resume/replay/diagnostic exposure paths

Requirements:
- produce a concise but real threat model artifact or section
- identify key attack surfaces
- identify mitigations implemented in this phase
- identify residual risks and deferred items

Update `README.md` and/or `project.md` accordingly.

### 13. Add security-focused tests

Add meaningful security-focused tests.

Required coverage should include, at minimum:
- encrypted token persistence behavior
- redaction of secrets in logs/events
- allow/deny command policy enforcement
- path traversal denial
- unsafe path denial through tool runtime
- file overwrite safeguard behavior
- approval requirement enforcement for destructive actions
- secret non-leakage in persisted memory or replay artifacts where practical

Also add tests where appropriate for:
- malformed or hostile input to RPC/CLI command paths
- symlink/path normalization edge cases
- missing/invalid encryption key behavior
- redaction edge cases for partial token-like strings
- denial audit logging behavior

Tests should validate the real execution paths, not only isolated helpers.

### 14. Error handling and safe failure behavior

Security controls must fail safely.

Requirements:
- missing encryption configuration should fail closed for persisted token storage
- denied commands/paths/actions should return explicit normalized errors
- redaction failures should not result in plaintext secret leakage where avoidable
- logging/diagnostic paths should prefer omission/redaction over accidental exposure
- avoid silent fallback to insecure behavior

### 15. Implement audit logging

Implement or improve audit logging for security-relevant actions.

At minimum capture where appropriate:
- approval-required action requested
- approval granted/denied
- destructive action attempted
- command denied by policy
- unsafe path denied
- token storage initialization/migration events
- auth/token lifecycle events without leaking secret values

Requirements:
- audit logs must be structured and useful
- do not store raw secrets in audit records
- associate records with session/run/operator context where available
- ensure audit logging supports later diagnostics and review

### 16. Keep security controls separate from UI surfaces

The security model must live below CLI/TUI/RPC/UI layers.

Requirements:
- approvals and denials must be enforced in runtime/core layers
- UI surfaces should reflect security decisions, not be the only place they happen
- avoid duplicating security logic across CLI, TUI, and RPC adapters
- keep policy and enforcement reusable and testable

### 17. Documentation updates

You must update:

#### `project.md`
Include:
- phase progress updates
- completed deliverables/tasks checkboxes
- architecture notes for security hardening
- threat model summary
- approval/permission policy notes
- secrets handling notes
- known limitations or deferred follow-up work

#### `README.md`
Include:
- overview of the security model
- secure token storage behavior
- key/configuration expectations
- command/path permission controls
- overwrite/destructive action safeguards
- approval behavior
- redaction behavior
- audit logging behavior
- security testing instructions
- caveats/limitations/residual risks

If helpful, add concise sections/tables showing:
- control
- what it protects
- where it is enforced
- whether it is configurable

---

## Implementation quality bar

- Build real security boundaries, not surface-level patches.
- Encrypt persisted tokens properly.
- Redact secrets before they leak into logs, events, memory, or replay.
- Bound destructive actions with policy and approval.
- Make path and command controls centralized and testable.
- Keep audit logs useful without exposing secrets.
- Fail safely when controls cannot be enforced.
- Avoid placebo security features.

## Deliverable expectations

When complete, provide:

1. A summary of what was implemented
2. The main files created or updated
3. The key architectural decisions
4. Any limitations or deferred follow-up work
5. Confirmation that `README.md` and `project.md` were updated

Do not stop at scaffolding. Complete the phase to a working standard consistent with the project’s architecture.