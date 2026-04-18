Complete **Phase 8 — Obsidian vault memory and personality system** for this project.

## Goal

Implement a human-readable memory layer backed by an Obsidian-style Markdown vault, define canonical personality/memory documents, add safe synchronization between structured runtime memory and vault memory, and load personality documents into prompt assembly in a stable, inspectable way.

## Critical instructions

- Fully inspect the current codebase and architecture before making changes.
- Preserve sound architectural patterns already present, but refactor where needed to support a clean long-term vault + memory system.
- Build this as real production-oriented infrastructure, not as a demo-only Markdown adapter.
- Do not leave fake vault sync logic, placeholder personality loading, or incomplete read/write contracts presented as complete.
- If prerequisite foundations from earlier phases are incomplete, implement the minimum required support so this phase is correct and integrated.
- Keep vault I/O, Markdown parsing, structured memory synchronization, retrieval indexing, personality loading, and prompt assembly clearly separated behind clean interfaces.
- Treat human-readable memory as a first-class system surface, with strong safety and traceability.
- Ensure you update both `README.md` and `project.md` to reflect architecture, configuration, canonical docs, runtime behavior, sync rules, limitations, and phase progress.
- When finished, mark completed items in `project.md` if that file tracks progress.
- Output a concise completion summary, the major files changed, key design decisions, and any deferred follow-up work.

---

# Phase 8 — Obsidian vault memory and personality system

## Objectives

Implement the human-readable memory layer and personality documents.

## Deliverables

- [ ] Obsidian vault integration
- [ ] Markdown memory schema
- [ ] personality document loader
- [ ] sync rules between runtime memory and vault memory

## Tasks

- [ ] Implement vault path configuration
- [ ] Implement Markdown memory reader/writer
- [ ] Implement canonical docs:
  - [ ] `AGENTS.md`
  - [ ] `BOOT.md`
  - [ ] `BOOTSTRAP.md`
  - [ ] `HEARTBEAT.md`
  - [ ] `IDENTITY.md`
  - [ ] `SOUL.md`
  - [ ] `TOOLS.md`
  - [ ] `USER.md`
- [ ] Define which docs are:
  - [ ] read-only at runtime
  - [ ] writable by runtime
  - [ ] writable only by approval
- [ ] Implement `memory-sync`
  - [ ] structured store → vault sync
  - [ ] vault → retrieval index sync
  - [ ] conflict rules
- [ ] Implement personality loading into prompt assembly
- [ ] Add tests for:
  - [ ] malformed markdown
  - [ ] sync conflicts
  - [ ] duplicate note handling
  - [ ] prompt assembly from personality docs

## Exit criteria

- [ ] The agent can load personality and long-term memory from the vault
- [ ] Runtime memory and vault memory can synchronize safely
- [ ] Human-readable memory is stable and inspectable

---

## Required implementation approach

### 1. Audit the existing architecture first

Before writing code:

- Review the current session/memory architecture, persistence layers, context engine, prompt assembly flow, configuration system, storage abstractions, eventing/audit behavior, and tests.
- Identify any existing memory abstractions, structured memory stores, retrieval indexing, Markdown parsing, filesystem safety rules, or prompt personality hooks.
- Determine the correct boundary for vault integration so the rest of the runtime depends on a clean abstraction, not raw filesystem access.
- Refactor fragmented memory or prompt-loading logic if needed so this phase results in one coherent system.

### 2. Define a clean vault memory architecture

Implement or refine an architecture with clearly separated layers for:

- vault path/config resolution
- filesystem-safe vault access
- Markdown parsing/serialization
- canonical document typing/schema
- structured runtime memory ↔ vault synchronization
- retrieval index sync
- conflict detection/resolution
- personality document loading
- prompt assembly integration

Do not collapse all of this into one ad hoc “vault helper.”

### 3. Implement vault path configuration

Implement configurable vault path support.

Requirements:
- support explicit vault path configuration through the project’s config system
- validate the configured vault path at startup or initialization time
- fail clearly for missing/inaccessible/invalid vault paths
- normalize path handling safely
- integrate with any existing path-safety rules from earlier phases
- document configuration clearly in `README.md`

If the project supports multiple environments, ensure vault configuration behaves predictably across them.

### 4. Implement Markdown memory reader/writer

Implement a real Markdown memory reader/writer for vault documents.

Requirements:
- read Markdown documents safely and consistently
- write Markdown deterministically so documents remain human-readable
- preserve useful formatting where practical
- avoid destructive rewrites where only small targeted changes are needed
- support metadata/frontmatter if appropriate for the project architecture
- normalize malformed or partially invalid docs into clear error behavior rather than silent corruption

This must be suitable for both human-maintained and runtime-maintained documents.

### 5. Define the Markdown memory schema

Define a clear Markdown memory schema for vault-backed memory.

Requirements:
- establish canonical structure expectations for the core documents
- support stable parsing and serialization
- distinguish between human-authored prose and structured machine-managed sections where needed
- make the schema inspectable and documented
- avoid overcomplicated hidden conventions that humans cannot reasonably maintain

If frontmatter, headings, fenced blocks, tags, or section markers are used, define them explicitly and document them clearly.

### 6. Implement the canonical docs

Implement support for these canonical documents:

- `AGENTS.md`
- `BOOT.md`
- `BOOTSTRAP.md`
- `HEARTBEAT.md`
- `IDENTITY.md`
- `SOUL.md`
- `TOOLS.md`
- `USER.md`

Requirements:
- define the purpose of each doc within the system
- define how each doc is loaded, parsed, and used
- create canonical templates/examples if the project architecture expects defaults
- ensure each document’s role is consistent and not overlapping in a confusing way

Suggested interpretation unless the existing architecture strongly suggests otherwise:
- `AGENTS.md`: project/agent operating instructions and behavioral conventions
- `BOOT.md`: short boot-time essentials for immediate runtime initialization
- `BOOTSTRAP.md`: deeper initialization guidance, setup rules, and environment bring-up notes
- `HEARTBEAT.md`: current status, ongoing priorities, recency-sensitive state, operational rhythm
- `IDENTITY.md`: agent identity, role, scope, tone anchors
- `SOUL.md`: hard rules, ethics, boundaries, non-negotiables, “baked into the soul”
- `TOOLS.md`: tool inventory, tool usage constraints, safety notes, approval expectations
- `USER.md`: durable user preferences, habits, expectations, and communication style

If the project already has semantics for these docs, align with those rather than forcing this exact interpretation.

### 7. Define runtime write policies for docs

Define and implement which docs are:

- read-only at runtime
- writable by runtime
- writable only by approval

Requirements:
- encode this policy in code, not only documentation
- make policy decisions explicit and inspectable
- block unauthorized runtime writes cleanly
- integrate with existing approval/sensitive-action hooks if present
- document the write rules for every canonical doc

A sensible default model is:
- high-identity and hard-rule docs are read-only or approval-gated
- current-state / operational docs may be runtime-writable
- user/personality/core-boundary docs should not be silently mutated

Do not let the runtime freely self-rewrite its core identity or hard rules.

### 8. Implement `memory-sync`

Implement a real `memory-sync` pipeline.

#### structured store → vault sync
Requirements:
- sync selected runtime memory/state into human-readable vault docs
- preserve readability and determinism
- avoid duplicate or runaway note growth
- make synced content traceable to its structured origin where useful

#### vault → retrieval index sync
Requirements:
- scan relevant vault documents/notes
- parse and normalize them
- update the retrieval index/semantic memory layer
- avoid indexing malformed or duplicate content incorrectly
- integrate with the current retrieval system from earlier phases

#### conflict rules
Requirements:
- define conflict behavior explicitly
- detect cases where both structured memory and vault content changed incompatibly
- choose a deterministic resolution strategy
- surface conflicts clearly for logs/tests and for approval/review where appropriate
- do not silently overwrite meaningful human edits

### 9. Define a conflict resolution model

Implement conflict rules that are explicit, safe, and documented.

Requirements:
- define what constitutes a conflict
- distinguish machine-managed vs human-managed sections if applicable
- support deterministic merge/reject behavior
- preserve data integrity and human trust
- make it obvious when manual intervention is needed

A strong MVP pattern is:
- machine-managed sections can be rewritten deterministically
- human-authored sections are preserved unless explicit approval exists
- conflicting edits create a conflict artifact/status rather than silent overwrite

### 10. Implement duplicate note handling

Duplicate or overlapping notes must be handled deliberately.

Requirements:
- detect duplicates or near-duplicates where feasible
- avoid repeated indexing of the same memory
- avoid duplicate sync writes into vault docs
- document how duplicates are detected and resolved
- ensure tests cover duplicate handling

### 11. Implement personality loading into prompt assembly

Implement personality loading so canonical personality docs feed prompt construction.

Requirements:
- define which docs participate in prompt assembly and in what order/priority
- keep this token-aware and compatible with the context engine from earlier phases
- preserve separation between hard rules, personality/identity, operational state, and user preferences
- avoid blindly injecting entire vault contents into every prompt
- support missing-doc fallbacks safely
- keep the assembly deterministic and testable

A likely pattern is:
- `SOUL.md` + `IDENTITY.md` + selected sections of `AGENTS.md` form the hard/behavior layer
- `USER.md` contributes durable user-specific preferences
- `BOOT.md` / `BOOTSTRAP.md` / `HEARTBEAT.md` contribute runtime/operational context as needed
- `TOOLS.md` contributes tool behavior constraints or available capabilities

### 12. Keep personality stable and inspectable

The human-readable personality system must remain stable.

Requirements:
- do not let runtime prompt assembly become opaque or magical
- expose enough metadata to know which docs/sections were loaded
- make precedence/merge behavior explicit
- avoid hidden prompt mutations that humans cannot inspect in the vault
- ensure that changing a canonical doc produces predictable downstream behavior

### 13. Error handling and fallback behavior

Implement sensible handling for failure cases.

Requirements:
- malformed Markdown should fail clearly without crashing unrelated runtime functions where possible
- missing canonical docs should use defined defaults or clear warnings/errors
- invalid write attempts should be blocked with explicit errors
- sync failures should be visible and not silently drop memory
- indexing failures should not corrupt existing retrieval state
- prompt assembly should degrade safely if a non-critical doc is missing or malformed

### 14. Testing

Add meaningful automated tests.

#### Required
- malformed markdown
- sync conflicts
- duplicate note handling
- prompt assembly from personality docs

#### Also add where appropriate
- vault path validation
- canonical doc parse/serialize round-trips
- runtime write policy enforcement
- structured store → vault sync correctness
- vault → retrieval index sync correctness
- missing doc fallback behavior
- deterministic prompt assembly ordering
- machine-managed section preservation
- human-authored section preservation

Tests should exercise the real vault/personality flow, not only isolated helpers.

### 15. Observability and auditability

Where appropriate within the current architecture, expose enough information to debug the system.

Useful metadata/logging may include:
- selected vault path
- loaded canonical docs
- sync direction and counts
- indexed note counts
- skipped/malformed/duplicate docs
- conflict detection results
- prompt personality inputs used
- blocked write attempts by policy

Do not overbuild dashboards in this phase, but make behavior diagnosable.

### 16. Documentation updates

You must update:

#### `project.md`
Include:
- phase progress updates
- completed deliverables/tasks checkboxes
- architecture notes for vault memory and personality loading
- canonical doc roles
- runtime write policy summary
- sync/conflict notes
- known limitations or deferred follow-up work

#### `README.md`
Include:
- overview of Obsidian vault integration
- vault configuration instructions
- Markdown memory schema overview
- descriptions of all canonical docs
- which docs are read-only, runtime-writable, or approval-gated
- how `memory-sync` works
- conflict resolution behavior
- retrieval index sync behavior
- personality loading behavior
- testing instructions
- caveats/limitations

If useful, add a concise section/table showing:
- document
- purpose
- runtime read/write policy
- included in prompt assembly or not

---

## Implementation quality bar

- Keep vault integration, sync, retrieval, and prompt assembly cleanly separated.
- Preserve human readability.
- Do not let the runtime self-modify its core identity or hard rules without policy.
- Make sync deterministic and safe.
- Make conflicts explicit.
- Make prompt personality assembly stable and inspectable.
- Avoid brittle Markdown parsing and destructive rewrites.
- Build this as a durable human + machine memory bridge, not a toy note reader.

## Deliverable expectations

When complete, provide:

1. A summary of what was implemented
2. The main files created or updated
3. The key architectural decisions
4. Any limitations or deferred follow-up work
5. Confirmation that `README.md` and `project.md` were updated

Do not stop at scaffolding. Complete the phase to a working standard consistent with the project’s architecture.