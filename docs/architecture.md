# rustpi Architecture

## Overview

`rustpi` is a Rust workspace with 13 crates organized in a strict dependency hierarchy. The crates
form a layered platform where lower layers define shared abstractions and higher layers compose them
into user-facing interfaces. At the foundation, `agent-core` and `config-core` define the shared
vocabulary (types, errors, configuration structures) that every other crate imports. Above them sit
infrastructure crates (`event-log`, `auth-core`, `policy-engine`) that provide durable storage, auth
flows, and policy evaluation. Mid-layer crates (`model-adapters`, `tool-runtime`, `context-engine`,
`session-store`, `memory-sync`) implement the agent's core capabilities against those abstractions.
A machine interface layer (`rpc-api`) exposes stdin/stdout JSONL RPC for programmatic control, and
two binary crates (`cli`, `tui`) compose everything into operator-facing interfaces.

---

## Crate Dependency Diagram

The diagram below shows the dependency flow bottom-up. Arrows point from dependents to dependencies
(i.e., upward = "depends on"). The actual `Cargo.toml` files are authoritative; this is a structural
guide.

```
                    ┌─────────┐   ┌─────────┐
                    │   cli   │   │   tui   │      ← Binaries (Phase 10, 11)
                    └────┬────┘   └────┬────┘
                         └──────┬──────┘
                    ┌───────────▼───────────┐
                    │        rpc-api        │      ← Machine interface (Phase 9)
                    └───────────┬───────────┘
          ┌──────────┬──────────┼──────────┬──────────┐
    ┌─────▼────┐ ┌───▼────┐ ┌──▼───────┐ ┌▼─────────┐ ┌──────────────┐
    │tool-     │ │context-│ │session-  │ │model-    │ │memory-sync   │
    │runtime   │ │engine  │ │store     │ │adapters  │ │              │
    └─────┬────┘ └───┬────┘ └──┬───────┘ └┬─────────┘ └──────┬───────┘
          └──────────┴──────┬──┴──────────┘          ┌────────┘
                    ┌───────▼───────┐         ┌───────▼────────┐
                    │ policy-engine │         │   auth-core    │    ← Auth (Phase 3)
                    └───────┬───────┘         └───────┬────────┘
                            └──────────┬──────────────┘
                          ┌────────────▼────────────┐
                          │        event-log         │   ← Event storage (Phase 2)
                          └────────────┬────────────┘
                    ┌──────────────────┼──────────────────┐
          ┌─────────▼─────────┐  ┌────▼────┐  ┌──────────▼──────────┐
          │    agent-core     │  │config-  │  │   (workspace deps)   │
          │  (shared types)   │  │  core   │  │ tokio · serde · uuid │
          └───────────────────┘  └─────────┘  └─────────────────────┘
```

**Key invariant:** `agent-core` and `config-core` have **no internal crate dependencies**. Every
other crate may depend on them freely. No crate may create a circular dependency.

Actual direct internal dependencies per crate (from `Cargo.toml`):

| Crate | Internal deps |
|---|---|
| `agent-core` | _(none)_ |
| `config-core` | `agent-core` |
| `event-log` | `agent-core` |
| `auth-core` | `agent-core` |
| `policy-engine` | `agent-core` |
| `model-adapters` | `agent-core`, `auth-core` |
| `tool-runtime` | `agent-core` |
| `context-engine` | `agent-core` |
| `session-store` | `agent-core` |
| `memory-sync` | `agent-core` |
| `rpc-api` | `agent-core` |
| `cli` | `agent-core`, `config-core`, `rpc-api` |
| `tui` | `agent-core`, `config-core`, `rpc-api` |

---

## Crate Reference

| Crate | Role | Key exports | Phase | Must not depend on |
|---|---|---|---|---|
| `agent-core` | Shared types and core errors — the vocabulary of the entire system | `RunId`, `SessionId`, `ProviderId`, `ModelId`, `ToolCall`, `ToolResult`, `AuthState`, `AuthFlow`, `AgentEvent`, `AgentError` | 0 | Any other internal crate |
| `config-core` | Layered runtime configuration — global, project, user, provider, memory, interface | `Config`, `GlobalConfig`, `ProviderConfig`, `MemoryConfig`, `ProviderKind`, `SessionBackend` | 0–2 | Any other internal crate |
| `event-log` | Append-only JSONL event storage and replay | `EventStore` trait, `encode_event`, `decode_event` | 0–2 | `model-adapters`, `tool-runtime`, `session-store`, `cli`, `tui` |
| `auth-core` | OAuth, device flow, API key auth; token refresh; encrypted persistence | `ProviderAuth` trait, `TokenStore` trait, `AuthError` | 0–3 | `model-adapters`, `tool-runtime`, `session-store`, `cli`, `tui` |
| `policy-engine` | Allow/deny/approval rule evaluation for tools, files, providers | `PolicyEngine`, `PolicyVerdict`, `PolicyRule`, `PolicyTarget`, `PolicyAction` | 0–2 | `model-adapters`, `tool-runtime`, `session-store`, `auth-core`, `cli`, `tui` |
| `model-adapters` | `ModelProvider` trait + per-provider implementations | `ModelProvider` trait, `CompletionRequest`, `CompletionResponse`, `TokenDelta`, `ProviderCapabilities`, `ProviderError`, `ChatMessage`, `Role`, `TokenUsage` | 0–4 | `tool-runtime`, `context-engine`, `session-store`, `cli`, `tui` |
| `tool-runtime` | Subprocess execution, file tools, policy gating, timeout/cancel | `Tool` trait, `ToolRegistry`, `ToolRunner`, `ToolError` | 0–5 | `model-adapters`, `context-engine`, `session-store`, `cli`, `tui` |
| `context-engine` | Filesystem scan, ignore rules, relevance scoring, context packing | `ContextScanner`, `ContextPacker`, `PackedContext`, `ScoredFile` | 0–6 | `model-adapters`, `tool-runtime`, `session-store`, `cli`, `tui` |
| `session-store` | Durable session/run/event persistence (SQLite, sled, PostgreSQL) | `SessionStore` trait, `RunStore` trait, `SessionRecord`, `RunRecord`, `RunStatus` | 0–7 | `model-adapters`, `tool-runtime`, `context-engine`, `cli`, `tui` |
| `memory-sync` | Obsidian vault ↔ structured store synchronization | `VaultAccessor`, `DocMutability`, `VAULT_DOCS`, `MemorySyncError` | 0–8 | `model-adapters`, `tool-runtime`, `cli`, `tui` |
| `rpc-api` | stdin/stdout JSONL RPC protocol for machine-to-machine control | `RpcRequest`, `RpcResponse`, `RpcMethod`, `RpcError` | 0–9 | `cli`, `tui` |
| `cli` | Binary `rustpi` — scriptable CLI interface | Binary entry point (`rustpi`) | 10 | `tui` |
| `tui` | Binary `rustpi-tui` — Ratatui interactive operator interface | Binary entry point (`rustpi-tui`) | 11 | `cli` |

---

## Shared Types Reference (`agent-core::types`)

These are the cross-cutting types imported by every crate. They form the shared vocabulary of the
entire runtime. All types derive `Debug`, `Clone`, `Serialize`, `Deserialize`.

### Identity types

- **`RunId`** — Newtype over `Uuid` (`RunId(pub Uuid)`). Identifies a single model completion run.
  Generated with `RunId::new()` → `Uuid::new_v4()`. Implements `Display` as the bare UUID string.
- **`SessionId`** — Newtype over `Uuid`. Identifies a conversation session, which contains one or
  more runs. Same construction pattern as `RunId`.
- **`ProviderId`** — Newtype over `String`. Names a model provider, e.g. `"openai"`,
  `"github-copilot"`, `"llamacpp"`. Constructed with `ProviderId::new(impl Into<String>)`.
- **`ModelId`** — Newtype over `String`. Names a specific model within a provider, e.g. `"gpt-4o"`,
  `"llama-3-70b"`. Constructed with `ModelId::new(impl Into<String>)`.

### Tool types

- **`ToolCall`** — A tool invocation emitted by the model. Fields: `id: String` (model-assigned),
  `name: String` (registered tool name), `arguments: serde_json::Value` (JSON-encoded params).
- **`ToolResult`** — The outcome of executing a `ToolCall`. Fields: `call_id: String` (must match
  `ToolCall::id`), `success: bool`, `output: serde_json::Value`.

### Auth types

- **`AuthState`** — Enum describing a provider's authentication status. Tagged with
  `#[serde(tag = "state", rename_all = "snake_case")]`. Variants:
  - `Unauthenticated` — no credentials present
  - `Pending { flow: AuthFlow, expires_at: DateTime<Utc> }` — auth challenge in progress
  - `Authenticated { provider: ProviderId, expires_at: Option<DateTime<Utc>> }` — valid credentials
  - `Expired { provider: ProviderId }` — token expired, refresh needed
  - `Failed { reason: String }` — auth failed, requires user action
- **`AuthFlow`** — Enum describing which auth mechanism is in use. Variants: `OAuthBrowser`,
  `DeviceCode`, `ApiKey`. Serialized `snake_case`.

### Event type

- **`AgentEvent`** — Top-level event enum. Serialized with
  `#[serde(tag = "type", rename_all = "snake_case")]` so every JSONL line includes a `"type"` field.
  Variant groups:
  - **Session lifecycle:** `SessionCreated`, `SessionResumed`, `SessionEnded`
  - **Run lifecycle:** `RunStarted`, `RunCompleted`, `RunCancelled`, `RunFailed`
  - **Model output:** `TokenChunk`, `ToolCallRequested`, `ToolResultSubmitted`
  - **Tool execution:** `ToolExecutionStarted`, `ToolStdout`, `ToolStderr`,
    `ToolExecutionCompleted`, `ToolExecutionFailed`, `ToolExecutionCancelled`
  - **Auth:** `AuthStateChanged`
  - **Context:** `ContextBuilt`, `ContextCompacted`

### Error type

- **`AgentError`** — Core error enum. Variants: `RunNotFound(RunId)`, `SessionNotFound(SessionId)`,
  `Cancelled`, `ContextBudgetExceeded(u32)`, `Serialization(serde_json::Error)`.

---

## Phase 0 Stub Inventory

The following components exist as compilable stubs in Phase 0. They define the correct types and
trait signatures but contain no real implementation. This table tracks what is deferred and when it
will be filled in.

| Component | Phase 0 status | Full implementation |
|---|---|---|
| `config-core` file loading/layering | Config types defined (`Config`, `GlobalConfig`, etc.), no loader or file I/O | Phase 2 |
| `event-log` storage backends | `EventStore` trait defined, `encode_event`/`decode_event` work; no backend (no file, no DB) | Phase 2 |
| `policy-engine` rule evaluation | `PolicyEngine` defined; `evaluate_tool` and `evaluate_file_mutation` default to `PolicyVerdict::Allow` | Phase 2 |
| `auth-core` OAuth/device flows | `ProviderAuth` trait defined with `login`, `refresh`, `revoke`, `current_state`; no HTTP client | Phase 3 |
| `auth-core` encrypted token storage | `TokenStore` trait defined with `load`, `save`, `delete`; no encryption, no backend | Phase 3 |
| `model-adapters` provider implementations | `ModelProvider` trait fully defined; no concrete adapters exist | Phase 4 |
| `tool-runtime` subprocess execution | `ToolRunner` dispatches through `ToolRegistry` with timeout; no subprocess spawning | Phase 5 |
| `tool-runtime` approval workflow | Policy gating hook point exists in runner; no approval UI or callback | Phase 5 |
| `context-engine` filesystem scanner | `ContextScanner::scan()` returns `Ok(vec![])` | Phase 6 |
| `context-engine` context packing | `ContextPacker::pack()` returns empty `PackedContext` | Phase 6 |
| `session-store` backend implementations | `SessionStore` and `RunStore` traits defined; no SQLite/sled/Postgres backends | Phase 7 |
| `memory-sync` vault read/write | `VaultAccessor` struct exists with `vault_path()`; no file I/O | Phase 8 |
| `rpc-api` framing and dispatch | `RpcRequest`, `RpcResponse`, `RpcMethod` types defined; no server loop or framing | Phase 9 |
| `cli` subcommands | `rustpi` binary entry point only; no subcommand parsing | Phase 10 |
| `tui` Ratatui panes | `rustpi-tui` binary entry point only; no Ratatui widgets or panes | Phase 11 |

---

## Event Serialization Format

All runtime events are stored as **JSONL** (newline-delimited JSON). The `event-log` crate provides
`encode_event` and `decode_event` for codec operations.

### Rules

1. Each line is a complete, self-contained JSON object. No pretty-printing; no multiline values.
2. The `"type"` field is always present and equals the `AgentEvent` variant name in `snake_case`
   (enforced by `#[serde(tag = "type", rename_all = "snake_case")]`).
3. All timestamps are ISO-8601 UTC, produced by `chrono::DateTime<Utc>` with serde.
4. Lines must **not** be modified or deleted after writing. The log is append-only.

### Example stream

```jsonl
{"type":"session_created","session_id":"01924c5a-1111-7000-8000-000000000001","timestamp":"2025-01-01T00:00:00Z"}
{"type":"run_started","run_id":"01924c5b-2222-7000-8000-000000000002","session_id":"01924c5a-1111-7000-8000-000000000001","provider":"openai","model":"gpt-4o","timestamp":"2025-01-01T00:00:01Z"}
{"type":"context_built","run_id":"01924c5b-2222-7000-8000-000000000002","token_count":1842,"file_count":7,"timestamp":"2025-01-01T00:00:01Z"}
{"type":"token_chunk","run_id":"01924c5b-2222-7000-8000-000000000002","delta":"Hello","timestamp":"2025-01-01T00:00:02Z"}
{"type":"tool_call_requested","run_id":"01924c5b-2222-7000-8000-000000000002","call":{"id":"call_abc","name":"read_file","arguments":{"path":"src/main.rs"}},"timestamp":"2025-01-01T00:00:03Z"}
{"type":"tool_execution_started","run_id":"01924c5b-2222-7000-8000-000000000002","call_id":"call_abc","tool_name":"read_file","timestamp":"2025-01-01T00:00:03Z"}
{"type":"tool_execution_completed","run_id":"01924c5b-2222-7000-8000-000000000002","call_id":"call_abc","exit_code":0,"timestamp":"2025-01-01T00:00:04Z"}
{"type":"tool_result_submitted","run_id":"01924c5b-2222-7000-8000-000000000002","result":{"call_id":"call_abc","success":true,"output":"fn main() { ... }"},"timestamp":"2025-01-01T00:00:04Z"}
{"type":"run_completed","run_id":"01924c5b-2222-7000-8000-000000000002","timestamp":"2025-01-01T00:00:10Z"}
{"type":"session_ended","session_id":"01924c5a-1111-7000-8000-000000000001","timestamp":"2025-01-01T00:00:10Z"}
```

### Type-to-field mapping (selected variants)

| `"type"` value | Key fields |
|---|---|
| `session_created` | `session_id`, `timestamp` |
| `session_resumed` | `session_id`, `timestamp` |
| `session_ended` | `session_id`, `timestamp` |
| `run_started` | `run_id`, `session_id`, `provider`, `model`, `timestamp` |
| `run_completed` | `run_id`, `timestamp` |
| `run_cancelled` | `run_id`, `timestamp` |
| `run_failed` | `run_id`, `reason`, `timestamp` |
| `token_chunk` | `run_id`, `delta`, `timestamp` |
| `tool_call_requested` | `run_id`, `call` (`{id, name, arguments}`), `timestamp` |
| `tool_result_submitted` | `run_id`, `result` (`{call_id, success, output}`), `timestamp` |
| `tool_execution_started` | `run_id`, `call_id`, `tool_name`, `timestamp` |
| `tool_stdout` | `run_id`, `call_id`, `data`, `timestamp` |
| `tool_stderr` | `run_id`, `call_id`, `data`, `timestamp` |
| `tool_execution_completed` | `run_id`, `call_id`, `exit_code` (nullable), `timestamp` |
| `tool_execution_failed` | `run_id`, `call_id`, `reason`, `timestamp` |
| `tool_execution_cancelled` | `run_id`, `call_id`, `timestamp` |
| `auth_state_changed` | `provider`, `state` (nested `AuthState` object), `timestamp` |
| `context_built` | `run_id`, `token_count`, `file_count`, `timestamp` |
| `context_compacted` | `run_id`, `tokens_before`, `tokens_after`, `timestamp` |

---

## Workspace Dependencies

All external crates are pinned in `[workspace.dependencies]` in the root `Cargo.toml`. Individual
crates inherit versions via `{ workspace = true }`.

| Crate | Purpose |
|---|---|
| `tokio` (features = full) | Async runtime |
| `tokio-stream` | Async stream utilities (used by `model-adapters` for streaming completions) |
| `serde` + `serde_json` | Serialization — all public types derive `Serialize`/`Deserialize` |
| `uuid` (features = v4, serde) | UUID generation for `RunId` / `SessionId` |
| `thiserror` | Error derivation macros |
| `anyhow` | Error propagation in binaries (`cli`, `tui`) |
| `tracing` + `tracing-subscriber` | Structured logging throughout |
| `chrono` (features = serde) | UTC timestamps on all events |
| `async-trait` | `#[async_trait]` for object-safe async trait methods |
| `futures` | Stream/future combinators (streaming completions in `model-adapters`) |
