# ADR-0001: Runtime Event Model

**Status:** Accepted

## Context

The agent runtime must track the full lifecycle of sessions, runs, tool executions, and auth state changes. This state needs to be observable for debugging, replayable for crash recovery, and streamable to external consumers (RPC, TUI). Multiple consumers need to observe the same events concurrently.

Several approaches were considered:
- Mutable shared state with locks — simple but not replayable or auditable
- Database-of-record with change polling — durable but high latency for streaming
- Append-only event log with in-memory fan-out — replayable, streamable, auditable

## Decision

Adopt an **append-only, event-sourced runtime model**:

1. All significant runtime transitions emit a structured `AgentEvent` variant (defined in `agent-core::types`).
2. Events are the source of truth for runtime state; mutable state structs are derived by replaying events.
3. Events are serialized as **JSONL** (one JSON object per line) for storage and streaming.
4. In-memory fan-out is handled by `tokio::sync::broadcast` channels; the `event-log` crate handles durable storage.
5. The `#[serde(tag = "type", rename_all = "snake_case")]` pattern is used so every stored event is self-describing.

`AgentEvent` covers six domains:
- Session lifecycle (`session_created`, `session_resumed`, `session_ended`)
- Run lifecycle (`run_started`, `run_completed`, `run_cancelled`, `run_failed`)
- Model output (`token_chunk`, `tool_call_requested`, `tool_result_submitted`)
- Tool execution (`tool_execution_started`, `tool_stdout`, `tool_stderr`, `tool_execution_completed`, `tool_execution_failed`, `tool_execution_cancelled`)
- Auth state (`auth_state_changed`)
- Context (`context_built`, `context_compacted`)

## Consequences

**Positive:**
- Full session replay for debugging and crash recovery
- External consumers (RPC, TUI) receive the same event stream
- Audit trail is a natural by-product
- New event types can be added without schema migrations

**Negative:**
- Deriving current state from events requires replay or a snapshot cache
- JSONL files grow unboundedly without a compaction strategy (addressed in Phase 12)
- Broadcast channels drop events if consumers are slow (requires buffering strategy in Phase 9)
