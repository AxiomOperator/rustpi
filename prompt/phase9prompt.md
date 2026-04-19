Complete **Phase 9 — RPC API** for this project.

## Goal

Provide a stable machine-to-machine RPC API over stdin/stdout JSONL so external hosts can drive the runtime, attach/detach sessions, start/stop runs, receive streamed events, inspect auth status, and discover capabilities in a way that is suitable for CLI and future editor integrations.

## Critical instructions

- Fully inspect the existing codebase and architecture before making changes.
- Preserve sound architectural patterns already present, but refactor where needed to support a clean long-term RPC boundary.
- Build this as real production-oriented runtime infrastructure, not as a thin demo transport.
- Do not leave fake protocol handlers, placeholder schemas, or partially wired event streams presented as complete.
- If prerequisite foundations from earlier phases are incomplete, implement the minimum required support so this phase is correct and integrated.
- Keep transport framing, protocol schemas, session lifecycle handling, run control, event streaming, auth/capability queries, and runtime integration clearly separated behind clean interfaces.
- Ensure the RPC API is stable enough to become the shared control surface for the CLI and future editor or host integrations.
- Ensure you update both `README.md` and `project.md` to reflect architecture, protocol schemas, commands, behavior, configuration, limitations, and phase progress.
- When finished, mark completed items in `project.md` if that file tracks progress.
- Output a concise completion summary, the major files changed, key design decisions, and any deferred follow-up work.

---

# Phase 9 — RPC API

## Objectives

Provide machine-to-machine embedding and streaming control.

## Deliverables

- [ ] stdin/stdout JSONL RPC protocol
- [ ] session attach/detach
- [ ] structured request/response model
- [ ] streamed event output

## Tasks

- [ ] Define RPC request schema
- [ ] Define RPC response schema
- [ ] Define streaming event schema
- [ ] Implement session attach/detach
- [ ] Implement run start/stop commands
- [ ] Implement tool passthrough events
- [ ] Implement auth-status queries
- [ ] Implement capability discovery
- [ ] Add integration tests with a stub host process

## Exit criteria

- [ ] External hosts can drive the runtime over JSONL
- [ ] Streaming output is stable
- [ ] RPC can be used by CLI and future editor integrations

---

## Required implementation approach

### 1. Audit the existing architecture first

Before writing code:

- Review the current runtime/session model, event bus, tool runtime, auth/provider integration, config system, CLI entrypoints, and test structure.
- Identify any existing host-runtime boundaries, stream/event abstractions, protocol-like code, or partial embedding hooks already present.
- Determine the correct boundary for the RPC layer so the runtime stays transport-agnostic above that boundary.
- Refactor fragmented control/event code if needed so this phase results in one coherent RPC surface.

### 2. Define a clean RPC architecture

Implement or refine an RPC architecture with clearly separated layers for:

- stdin/stdout transport framing
- JSONL parsing/serialization
- request dispatch
- response generation
- streamed event emission
- session attach/detach lifecycle
- run control
- tool passthrough event forwarding
- auth-status queries
- capability discovery
- error normalization

Do not tightly couple business/runtime logic directly to stdin/stdout I/O primitives.

### 3. Define the RPC protocol framing

Implement the protocol over **stdin/stdout JSONL**.

Requirements:
- one JSON object per line
- robust line-delimited framing
- clean handling of partial/incomplete lines
- deterministic serialization
- no non-protocol noise written to stdout
- ensure protocol data and logs/diagnostics do not corrupt each other
- stderr may be used for local diagnostics only if that matches the project architecture and is documented

The transport must be suitable for long-lived host control sessions.

### 4. Define the RPC request schema

Define a structured request schema.

At minimum, requests should include:
- request ID / correlation ID
- method/command name
- params/payload
- optional session/run targeting fields where appropriate
- protocol version if appropriate for stability

Requirements:
- validate requests cleanly
- reject malformed/unknown methods explicitly
- keep the schema stable and extensible
- avoid ad hoc per-command request shapes that are impossible to maintain

### 5. Define the RPC response schema

Define a structured response schema.

At minimum, responses should support:
- request ID / correlation ID
- success/error result shape
- structured data payload
- normalized error object when a request fails

Requirements:
- responses must be machine-usable and predictable
- keep response envelopes consistent across commands
- distinguish protocol errors, validation errors, runtime errors, and unsupported-capability errors cleanly
- document all schemas in `README.md`

### 6. Define the streaming event schema

Define a structured streaming event schema for asynchronous/runtime-driven output.

At minimum, events should support:
- event type
- timestamp
- session ID and/or run ID where relevant
- event payload
- event source/category
- sequencing metadata if needed by the architecture

The schema must support:
- runtime status events
- token/text/content stream events if the runtime emits them
- tool passthrough events
- lifecycle events
- errors surfaced during asynchronous execution

Requirements:
- event shape must be stable and consistent
- event types must be explicit and documented
- event payloads must not leak random internal implementation structures

### 7. Implement session attach/detach

Implement RPC commands for session attach/detach.

Requirements:
- external hosts can attach to an existing session
- external hosts can detach cleanly without corrupting session state
- attaching should surface enough state/metadata for the host to understand context
- detaching should not accidentally destroy durable session state unless explicitly intended
- behavior for missing or invalid session IDs must be normalized and documented

If the runtime already has session lifecycle semantics, align with them rather than inventing conflicting behavior.

### 8. Implement run start/stop commands

Implement run control commands over RPC.

Requirements:
- external hosts can start a run
- external hosts can stop/cancel a run
- start/stop integrate with the runtime’s existing cancellation and eventing model
- starting a run should stream events through the defined event schema
- stopped/cancelled runs must report final state consistently
- concurrent/invalid run state transitions must fail cleanly

Do not implement this as a one-off path separate from the normal runtime flow.

### 9. Implement tool passthrough events

Implement passthrough/forwarding of tool lifecycle events through the RPC stream.

Requirements:
- forward relevant tool runtime events into the RPC streaming event model
- preserve important lifecycle semantics such as started/stdout/stderr/completed/cancelled/failed where applicable
- normalize tool events so hosts do not need direct knowledge of internal tool runtime structures
- ensure these events remain useful for CLI and future editor integrations

### 10. Implement auth-status queries

Implement RPC support for auth-status queries.

Requirements:
- external hosts can query current auth/provider auth state relevant to the runtime
- keep auth-status responses structured and safe
- expose enough information for hosts to decide whether user action is needed
- do not leak secrets, tokens, or sensitive raw credentials
- normalize unavailable/unconfigured provider auth state clearly

This is status/inspection, not credential exfiltration.

### 11. Implement capability discovery

Implement capability discovery over RPC.

Requirements:
- external hosts can ask what the runtime supports
- include useful capability metadata such as:
  - supported RPC methods
  - protocol version
  - available feature flags/capabilities
  - tool/runtime streaming support
  - auth/provider capability summaries where appropriate
- keep discovery machine-usable and stable
- document how hosts should use capability discovery for compatibility

This should be the basis for CLI/editor feature negotiation later.

### 12. Error handling and normalization

Implement a shared RPC/protocol error model.

Cover at minimum:
- malformed JSONL
- invalid request schema
- unknown method
- unsupported capability
- missing session
- invalid run state
- attach/detach failure
- runtime execution error
- cancellation/stop failure
- internal transport/dispatch failure

Requirements:
- preserve useful debug details internally where appropriate
- expose a stable, structured external error shape
- do not crash the transport loop because of one bad request unless the condition is unrecoverable

### 13. Keep transport and runtime decoupled

The RPC layer should adapt the runtime, not become the runtime.

Requirements:
- keep stdin/stdout framing separate from runtime orchestration
- keep protocol schemas separate from internal domain models where appropriate
- avoid spreading JSONL-specific conditionals throughout the codebase
- ensure future transports could be added later without rewriting runtime logic

### 14. Integration tests with a stub host process

Add integration tests with a stub host process.

Required coverage:
- start the runtime as a child/stubbed process
- send JSONL requests
- receive structured responses
- receive streamed events
- verify session attach/detach
- verify run start/stop
- verify tool passthrough events
- verify auth-status queries
- verify capability discovery
- verify malformed request handling
- verify stable behavior under streaming output

Tests should exercise the real transport/protocol boundary, not only internal dispatch helpers.

### 15. Stability requirements for streaming output

Streaming must be stable enough for external hosts.

Requirements:
- no interleaving of invalid protocol lines
- deterministic event envelopes
- clean correlation between responses and streamed events
- long-running runs continue to emit parseable JSONL
- final states are visible and unambiguous
- host disconnect or broken pipe behavior is handled safely and documented

### 16. Documentation updates

You must update:

#### `project.md`
Include:
- phase progress updates
- completed deliverables/tasks checkboxes
- architecture notes for the RPC API
- protocol/versioning notes
- session and run lifecycle notes
- capability/auth query notes
- known limitations or deferred follow-up work

#### `README.md`
Include:
- overview of the RPC API
- stdin/stdout JSONL framing rules
- request schema
- response schema
- streaming event schema
- supported methods/commands
- session attach/detach behavior
- run start/stop behavior
- tool passthrough behavior
- auth-status query behavior
- capability discovery behavior
- integration/testing instructions
- caveats/limitations

If helpful, add concise examples showing:
- one request line
- one response line
- one event line
- typical attach/start/stream/stop flow

---

## Implementation quality bar

- Keep protocol and runtime concerns cleanly separated.
- Build a stable machine-usable API, not a CLI hack.
- Make schemas explicit and testable.
- Make streaming robust and parseable.
- Normalize errors carefully.
- Make the RPC layer suitable for reuse by CLI and future editor integrations.
- Avoid leaking raw internal implementation structures across the protocol boundary.

## Deliverable expectations

When complete, provide:

1. A summary of what was implemented
2. The main files created or updated
3. The key architectural decisions
4. Any limitations or deferred follow-up work
5. Confirmation that `README.md` and `project.md` were updated

Do not stop at scaffolding. Complete the phase to a working standard consistent with the project’s architecture.