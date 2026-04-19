# rustpi

A native Rust AI agent platform with multi-provider model access, durable sessions, Obsidian-backed local-first memory, and a rich terminal UI.

**Status: Phases 0–14 complete — full-system testing and parity validation done**

---

## What rustpi is

rustpi is a fully local, operator-controlled AI agent runtime written in Rust. It provides:

- **Multi-provider LLM access** — OpenAI-compatible endpoints, local llama.cpp/vLLM, GitHub Copilot, Gemini; unified behind a single `ModelProvider` trait
- **Structured session management** — append-only event log, reproducible runs, full replay support
- **Local-first memory** — human-readable Obsidian vault integration; vector memory via Qdrant
- **Safe tool execution** — policy-gated subprocess runner with timeout, cancellation, and audit logging
- **Layered configuration** — global → user → project → runtime override precedence; TOML-backed
- **Shared auth subsystem** — OAuth browser flow, RFC 8628 device code flow, API key storage, encrypted token persistence, token refresh
- **CLI + Ratatui TUI** — scriptable output modes and a full-screen terminal interface

---

## Architecture

rustpi is a Cargo workspace of 14 focused library crates and two binary entry points. The dependency graph flows strictly upward — primitive types live at the bottom, binaries at the top, with no circular dependencies.

```
cli / tui  (binaries)
    │
rpc-api ──────────────────────────────────┐
    │                                     │
model-adapters  tool-runtime  context-engine  memory-sync
    │               │
auth-core       policy-engine     observability
    │               │                   │
session-store   event-log    config-core
    └───────────────┴────────────┘
                    │
              agent-core  (shared types, AgentEvent, core traits)
```

### Crate reference

| Crate | Role | Status |
|---|---|---|
| `agent-core` | Shared types, `AgentEvent` hierarchy, core traits; **`Redactor`** (secrets redaction) | ✅ Phase 1/13 |
| `config-core` | Layered config (global/user/project/runtime), TOML loading, merge rules | ✅ Phase 2 |
| `policy-engine` | Allow/deny/approval rule evaluation with glob matching; **glob-based tool/file/provider/auth policy** | ✅ Phase 2/13 |
| `event-log` | Append-only JSONL event log, replay reader, audit records | ✅ Phase 2 |
| `auth-core` | OAuth browser flow, RFC 8628 device flow, API key path, **AES-256-GCM `EncryptedFileTokenStore`**, refresh | ✅ Phase 3/13 |
| `model-adapters` | `ModelProvider` trait, provider registry; OpenAI-compatible, llama.cpp, vLLM, and GitHub Copilot adapters | ✅ Phase 4 |
| `tool-runtime` | Tool trait, registry, subprocess runner with timeout; **`CommandPolicy`**, **`OverwritePolicy`**, **`AuditSink`**, **`PathSafetyPolicy`** | ✅ Phase 5/13 |
| `context-engine` | Context window assembly, ignore rules, token budgeting | ✅ Phase 6 |
| `session-store` | Durable session persistence (SQLite / sled / PostgreSQL); 4 store traits + config-driven factory; crash recovery | ✅ Phase 7/12 |
| `memory-sync` | Qdrant semantic memory (`QdrantMemory`); Obsidian vault reader/writer, canonical docs, personality loader, sync engine | ✅ Phase 8 |
| `rpc-api` | stdin/stdout JSONL RPC server — session attach/detach, run start/cancel, auth status, capabilities | ✅ Phase 9 |
| `cli` | Scriptable CLI binary (`rustpi`); `replay` and enhanced `diag` commands | ✅ Phase 10/12 |
| `tui` | Ratatui full-screen TUI binary (`rustpi-tui`) | ✅ Phase 11 |
| `observability` | `TelemetryCollector`, `ProviderMetrics`, `TokenUsageTracker`, `ToolMetrics`, `TelemetrySummary` | ✅ Phase 12 |

---

## Quick start

```sh
git clone <repo>
cd rustpi
cargo build --workspace
cargo test --workspace
```

### Running with a provider

1. Add a provider to `~/.config/rustpi/config.toml`:
   ```toml
   [[providers]]
   id = "local"
   kind = "openai_compatible"
   base_url = "http://localhost:11434/v1"  # Ollama example
   ```

2. Run a prompt:
   ```sh
   rustpi run "explain this codebase"
   ```

3. Open the TUI:
   ```sh
   rustpi chat
   ```

4. Check status:
   ```sh
   rustpi status
   ```

5. List past sessions:
   ```sh
   rustpi sessions
   ```

### Requirements

- Rust 1.75+ (stable)
- No additional system dependencies for the core runtime
- Qdrant (optional, for vector memory — Phase 7+, enabled via `qdrant_enabled = true` in config)
- An Obsidian vault (optional, for human-readable memory — Phase 8+)

---

## Configuration

rustpi uses a layered TOML configuration system. Each layer overrides the one below it:

| Layer | Default path | Scope |
|---|---|---|
| Defaults | (compiled in) | Baseline values |
| Global | `~/.config/rustpi/config.toml` | Machine-wide |
| User | `~/.rustpi/config.toml` | Per-user preferences |
| Project | `.rustpi/config.toml` (cwd) | Per-project overrides |
| Runtime | Flags / env vars | Per-invocation |

A minimal project config:

```toml
[global]
default_provider = "openai"
default_model = "gpt-4o"

[[providers]]
id = "openai"
kind = "open_ai_compatible"
base_url = "https://api.openai.com/v1"

[providers.auth]
kind = "api_key"
env_var = "OPENAI_API_KEY"
```

---

## Auth flows

rustpi supports three auth modes, selectable per-provider:

| Flow | Use case |
|---|---|
| **API key** | OpenAI, Anthropic, and any static-key provider; resolved from env var or config |
| **OAuth browser** | Providers with browser-based consent (Gemini, etc.) |
| **Device code** (RFC 8628) | Headless / SSH environments; displays user code + verification URL |

Tokens are persisted encrypted at rest (AES-256-GCM) in `~/.config/rustpi/tokens.enc`. Refresh is handled automatically before expiry.

---

## Supported Providers

| Provider | Auth | Chat | Streaming | Embeddings | Model Discovery |
|---|---|---|---|---|---|
| OpenAI-compatible | API key | ✅ | ✅ | ✅ | ✅ |
| llama.cpp | None (local) | ✅ | ✅ | ⚙️ opt-in | ✅ active model |
| vLLM | None / API key | ✅ | ✅ | ✅ | ✅ |
| GitHub Copilot | OAuth device flow | ✅ | ✅ | ❌ | Static list |
| Gemini | OAuth / API key | 🔲 Planned | 🔲 Planned | 🔲 Planned | 🔲 Planned |

---

## Tool Runtime

The `crates/tool-runtime` crate provides a unified, safe, observable, policy-gated tool execution engine.

### Supported Tool Categories

| Tool | Name | Sensitivity | Streaming | Timeout | Cancellation | Approval Required |
|------|------|-------------|-----------|---------|--------------|-------------------|
| Shell command | `shell` | Critical | Yes | Yes | Yes | Always |
| Read file | `read_file` | Safe | No | Yes | Yes | No |
| Write file | `write_file` | High | No | Yes | Yes | Configurable |
| Text search | `search` | Safe | No | Yes | Yes | No |
| File edit | `edit_file` | High | No | Yes | Yes | Configurable |

### Lifecycle Events

Every tool execution emits these `AgentEvent` variants in order:

1. `ToolStarted` — tool name + invocation ID
2. `ToolStdout` / `ToolStderr` — incremental output lines (subprocess only)
3. `ToolCompleted` — success + optional exit code
4. `ToolCancelled` — if cancelled mid-run
5. `ToolFailed` — with reason string

### Timeout & Cancellation

- Default timeout: 30 seconds (configurable per invocation via `ToolConfig`)
- Cancellation via `tokio_util::sync::CancellationToken` — pass in `ToolConfig`
- Both timeout and cancellation emit the correct lifecycle event and return an error

### Approval Hooks

Built-in approval hook implementations:

- `AutoApprove` — approves all (use for tests/dev)
- `DenyAbove { threshold }` — denies tools at or above a sensitivity level
- `AllowList` — approves only named tools

Implement the `ApprovalHook` trait for custom policy.

### Path Safety

File and search/edit tools enforce path safety via `PathSafetyPolicy`:

- All paths are validated against configured allowed roots
- `..` traversal is blocked
- Explicit deny list supported
- Paths outside allowed roots → `ToolError::PathTraversal`

```rust
let policy = PathSafetyPolicy::new(["/workspace", "/tmp/scratch"]);
```

### Subprocess Streaming

The shell tool streams stdout/stderr as `ToolStdout`/`ToolStderr` events while the process runs. Output is capped at 512 KB per stream.

### Configuration Example

```rust
use tool_runtime::{
    runner::ToolRunner,
    approval::DenyAbove,
    schema::{ToolConfig, ToolSensitivity},
    tools::{shell::ShellTool, file::{ReadFileTool, WriteFileTool}},
    path_safety::PathSafetyPolicy,
};
use std::sync::Arc;
use std::time::Duration;

// Deny Critical tools (shell) by default
let approval = Arc::new(DenyAbove { threshold: ToolSensitivity::High });

// Build runner with event bus
let (event_tx, _event_rx) = tokio::sync::broadcast::channel(64);
let runner = ToolRunner::new(registry, Duration::from_secs(30))
    .with_approval(approval)
    .with_event_tx(event_tx);

// Register tools
let policy = Arc::new(PathSafetyPolicy::new(["/workspace"]));
runner.register(Arc::new(ReadFileTool::new(policy.clone())));
runner.register(Arc::new(WriteFileTool::new(policy.clone())));
```

### Testing the Tool Runtime

```bash
# Run all tool-runtime tests
cargo test -p tool-runtime

# Run only integration tests
cargo test -p tool-runtime --test tool_runtime_integration

# Run with output for debugging
cargo test -p tool-runtime -- --nocapture
```

---

## Persistence & Memory

### Session Store

The `crates/session-store` crate provides durable persistence for sessions, runs, summaries, and structured memory records behind four async traits:

| Trait | Domain |
|---|---|
| `SessionStore` | Session lifecycle — create, list, update summary, delete |
| `RunStore` | Run lifecycle within a session — status transitions (`Running` → `Completed` / `Cancelled` / `Failed`) |
| `SummaryStore` | Compaction artifacts — ordered summary records per session |
| `MemoryStore` | Structured memory records — tagged, searchable, optional session scope |

### Backend Options

| Backend | Use Case | Notes |
|---------|----------|-------|
| SQLite | Local/default | File-based, zero config, production-ready |
| sled | Embedded alternative | Pure Rust, fast, slightly different semantics |
| PostgreSQL | Production/multi-process | Requires connection URL |

All three backends implement all four traits. The factory layer in `session-store/src/factory.rs` constructs the correct `Arc<dyn Trait>` from config — callers never reference a concrete backend type.

### Migration/Versioning

SQLite and PostgreSQL backends store a schema version in a `_meta` / metadata table on first initialisation. Every subsequent open checks the version; a mismatch returns a `StoreError::Migration` before any data access. `SledBackend` uses tree naming conventions for forward compatibility. Schema migration is automatic on startup — no manual migration step is required.

### Configuration

```toml
[memory]
session_backend = "sqlite"           # sqlite | sled | postgres
postgres_url = "postgres://..."      # required for postgres
qdrant_enabled = false
qdrant_url = "http://localhost:6334"
```

Default paths when not specified:
- SQLite: `~/.rustpi/sessions.db`
- sled: `~/.rustpi/sessions.sled`

### Qdrant Semantic Memory

`QdrantMemory` in `crates/memory-sync` implements the `MemoryRetriever` trait from `context-engine`, enabling the context pipeline to pull relevant memory snippets into the prompt window.

- Connects to a Qdrant instance at a configurable URL (default collection `rustpi_memory`, vector size 1536 for OpenAI-compatible embeddings)
- **Phase 7:** retrieval uses scroll + keyword filtering (no embeddings yet)
- **Phase 9+:** `upsert_memory()` and `search_similar()` accept pre-computed embeddings; ANN search will replace scroll once an embedding model is wired in
- **Graceful offline fallback:** if Qdrant is unreachable, `retrieve()` logs a warning and returns an empty snippet list — the agent continues without memory

### Memory Abstraction

Two interfaces cover different memory concerns:

- **`MemoryStore`** — structured, tagged records with optional session scope; backed by the same SQLite/sled/Postgres backends as sessions and runs
- **`MemoryRetriever`** (`context_engine::memory`) — context-engine integration; `QdrantMemory` implements this for semantic retrieval; `NoopMemory` and `StaticMemory` are available for testing

### Testing

```bash
# Run session-store tests (SQLite + sled)
cargo test -p session-store

# Run memory-sync tests
cargo test -p memory-sync

# Run full workspace
cargo test --workspace

# Postgres tests require a live DB (marked #[ignore] by default)
DATABASE_URL=postgres://... cargo test -p session-store -- --ignored
```

---

## Vault Memory & Personality

The `crates/memory-sync` crate implements an Obsidian-style Markdown vault as the human-readable memory layer — a directory of plain `.md` files the operator can read and edit directly. The runtime reads personality from it, writes operational state back into it, and preserves every human-authored note.

### Overview

The vault provides two things:

- **Personality** — Soul, Identity, Agents, Boot, and User docs are loaded on startup and injected into the system prompt.
- **Operational state** — Heartbeat and Tools docs are updated by the runtime after each run to reflect current status.

Human-authored sections are **never** overwritten. Only sections explicitly tagged `<!-- machine-managed -->` may be updated by the runtime.

### Configuration

```toml
[memory]
obsidian_vault_path = "/path/to/your/vault"
```

The vault must be an existing directory. `VaultAccessor::open()` returns an error if the path is absent or is not a directory.

### Canonical Documents

| Document | Purpose | Runtime Policy | In Prompt |
|----------|---------|----------------|-----------|
| `SOUL.md` | Core rules, ethics, non-negotiables | Read-only | ✅ (priority 0) |
| `IDENTITY.md` | Agent identity, role, tone | Read-only | ✅ (priority 1) |
| `AGENTS.md` | Operating instructions, behavioral conventions | Read-only | ✅ (priority 2) |
| `BOOT.md` | Boot-time essentials | Read-only | ✅ (priority 3) |
| `BOOTSTRAP.md` | Deep initialization guidance | Read-only | ❌ |
| `HEARTBEAT.md` | Current status, operational state | Runtime-writable | ❌ |
| `TOOLS.md` | Tool inventory and constraints | Runtime-writable | ❌ |
| `USER.md` | User preferences and communication style | Approval-required | ✅ (priority 4) |

### Markdown Schema

Every vault document follows a simple schema:

- **Frontmatter** — optional YAML key/value pairs between `---` delimiters at the top of the file.
- **Sections** — `#` headings split the document into named sections. Each section owns the content lines below its heading until the next heading.
- **Machine-managed marker** — a `<!-- machine-managed -->` comment on the first line of a section's body marks that section as runtime-owned. The runtime may update it freely.
- **Human-authored sections** — all sections without the marker are preserved unconditionally; `upsert_machine_section()` will never touch them.

### Memory Sync

`SyncEngine` drives three operations:

- `sync_to_vault()` — pushes current runtime state into the `<!-- machine-managed -->` sections of `HEARTBEAT.md` and `TOOLS.md`.
- `index_vault()` — scans all vault documents and builds an in-memory index for retrieval.
- `detect_conflicts()` — inspects machine-managed sections for manual edits since the last sync; records a `RequiresReview` conflict when both the runtime and a human have changed the same section.

### Conflict Resolution

| Situation | Behaviour |
|-----------|-----------|
| Section marked `<!-- machine-managed -->` | Runtime may overwrite freely |
| Section without marker (human-authored) | Always preserved; never overwritten |
| Machine-managed section manually edited | `detect_conflicts()` records a `RequiresReview` conflict |

### Personality Loading

`load_personality()` assembles a token-bounded personality string from the prompt-included docs in priority order:

1. **Soul** (priority 0)
2. **Identity** (priority 1)
3. **Agents** (priority 2)
4. **User** (priority 3)
5. **Boot** (priority 4)

- Default token budget: **4,000 tokens** (shared across all personality docs).
- Docs that do not exist are silently skipped and listed in `PersonalityBundle::missing_docs`.
- `inject_personality()` adds the assembled text as a System section to `PromptAssembler`.

### Vault Initialization

On first use, `init_defaults()` creates minimal template files for any missing canonical docs:

```rust
let vault = VaultAccessor::open("/path/to/vault")?;
let created = vault.init_defaults()?;
// created: list of CanonicalDoc variants that were written
```

This is called automatically by the runtime — no manual setup step is required.

### Testing

```bash
cargo test -p memory-sync
cargo test --workspace
```

---

## Context Engine

The `crates/context-engine` crate assembles a token-bounded, relevance-ranked prompt context from the working directory, applying ignore rules, scoring heuristics, working-set selection, memory retrieval, and compaction.

### Component table

| Component | Purpose | Key Inputs | Key Outputs |
|-----------|---------|-----------|-------------|
| `Scanner` | Recursive filesystem discovery | Project root, `ScannerConfig` | `Vec<FileEntry>`, `ScanStats` |
| `IgnoreEngine` | Filter excluded paths | `.gitignore` + `.contextignore` files | Pass/ignore verdict per path |
| `relevance` | Score files for the current task | `Vec<FileEntry>`, `RelevanceHints` | `Vec<ScoredEntry>` (score 0.0–1.0) |
| `workset` | Token-budget-aware file selection | `Vec<ScoredEntry>`, `WorksetConfig` | `WorkingSet` (selected + excluded) |
| `compactor` | Reduce context when over budget | `Vec<SelectedFile>`, token budget, strategy | Compacted file list + summary string |
| `MemoryRetriever` | Retrieve relevant memory snippets | `MemoryQuery` (keywords + budget) | `Vec<MemorySnippet>` |
| `ContextPacker` | Assemble file blocks into token-bounded context | `Vec<SelectedFile>`, `Vec<MemorySnippet>`, `PackerConfig` | `PackedContext` |
| `ContextEngine` | Orchestrate the full pipeline | `EngineConfig`, `RelevanceHints`, optional `MemoryQuery` | `(PackedContext, EngineStats)` |
| `tokens` | Token estimation utilities | `&str` or byte count | Estimated token count, `Budget` tracker |

### Pipeline flow

```
scan() → ignore filter → relevance score → workset select
  ↓
compact (if estimated tokens > budget × compaction_threshold)
  ↓
memory retrieve
  ↓
pack (token budget) → PackedContext
```

### Scanning

`Scanner` walks the project root recursively, collecting `FileEntry` values (path, byte size, last-modified time). Configured via `ScannerConfig`:

```rust
ScannerConfig {
    max_files: 1000,       // hard limit on files scanned
    max_file_bytes: 512 * 1024,  // skip files larger than this
    follow_symlinks: false,
    ..Default::default()
}
```

### Ignore behavior

`IgnoreEngine` wraps the [`ignore`](https://crates.io/crates/ignore) crate (the same engine used by ripgrep), which automatically respects `.gitignore`, `.git/info/exclude`, and global gitignore. `.contextignore` is overlaid on top using the same glob semantics, giving projects fine-grained control over what enters the context window.

### Relevance scoring

`score()` and `score_all()` assign a `f32` score (0.0–1.0) to each file based on:
- **Extension** — source files score higher than build artifacts or binaries
- **Path proximity** — files closer to the project root or matching hint paths score higher
- **Keyword hints** — `RelevanceHints::keywords` boost files whose paths contain hint terms
- **Hint paths** — explicitly listed paths receive a maximum score boost

```rust
let hints = RelevanceHints {
    keywords: vec!["auth".into(), "token".into()],
    hint_paths: vec![PathBuf::from("src/auth.rs")],
    root: Some(project_root.clone()),
    ..Default::default()
};
```

### Working-set selection

`select()` picks files greedily by score (descending) while respecting the token budget and a **diversity cap** (`max_per_dir`) that prevents any single directory from dominating the working set:

```rust
WorksetConfig {
    max_files: 50,
    token_budget: 48_000,   // slightly over engine budget to leave room for compaction
    min_score: 0.0,
    max_per_dir: 10,        // at most 10 files from any one directory
}
```

### Token budgeting

Token counts use the `~4 chars/token` heuristic (`tokens::estimate()`), consistent with `agent_core::prompt::estimate_tokens`. The `Budget` struct tracks usage:

```rust
let mut budget = Budget::new(32_000);
if budget.would_fit(file_tokens) {
    budget.consume(file_tokens);
}
// budget.remaining(), budget.used, budget.is_exhausted()
```

Provider-specific byte-exact tokenization is deferred to Phase 7.

### Compaction strategies

If the estimated token total of the working set exceeds `token_budget × compaction_threshold` (default 1.5×), the compactor runs **before** file I/O:

| Strategy | Behaviour |
|----------|-----------|
| `DropLow { threshold }` | Drop files with score below threshold (default) |
| `ExtractDeclarations` | Rule-based extraction of function/type signatures (placeholder — LLM summarization deferred) |
| `Truncate` | Proportional tail removal per file |

A human-readable `compact_summary` is attached to the `PackedContext` when compaction runs.

### Memory retrieval

The `MemoryRetriever` trait abstracts memory backends:

```rust
#[async_trait]
pub trait MemoryRetriever: Send + Sync {
    async fn retrieve(&self, query: &MemoryQuery) -> Vec<MemorySnippet>;
}
```

Built-in implementations:

| Implementation | Behaviour |
|---|---|
| `NoopMemory` | Always returns empty (default) |
| `StaticMemory` | Returns a fixed pre-loaded list of snippets |
| `VaultMemory` | Reads `.md` files from an Obsidian vault directory (Phase 8 stub — no semantic search) |

### Configuration

```rust
use context_engine::{ContextEngine, EngineConfig, RelevanceHints};
use std::sync::Arc;

let config = EngineConfig {
    project_root: PathBuf::from("/workspace/myproject"),
    token_budget: 32_000,
    memory_budget: 2_000,
    max_scan_files: 1000,
    max_workset_files: 50,
    min_relevance_score: 0.0,
    max_per_dir: 10,
    compaction_threshold: 1.5,
};

let engine = ContextEngine::new(config)
    .with_memory(Arc::new(my_memory_backend));

let (packed_ctx, stats) = engine
    .build_context(hints, None)
    .await?;

// packed_ctx.render() → prompt-ready string
// stats.total_tokens, stats.compacted, stats.truncated
```

### Testing

```bash
# Run all context-engine tests (30 unit tests)
cargo test -p context-engine

# Run with output for debugging
cargo test -p context-engine -- --nocapture
```

---

## Policy model

All runtime actions pass through the `policy-engine` crate before execution. Rules are evaluated in order with first-match-wins semantics:

```toml
# Example: deny shell tools, require approval for file writes
[[policy.rules]]
name = "no-shell"
target = { tool = { name_glob = "shell_*" } }
action = "deny"

[[policy.rules]]
name = "file-write-approval"
target = { file_path = { path_glob = "/home/**" } }
action = "require_approval"
```

Possible verdicts: `allow` · `deny` · `require_approval`

---

## Event model

Every significant runtime action emits a structured `AgentEvent` to the append-only event log. Events are serialised as JSONL (one self-describing JSON object per line):

```json
{"type":"run_started","run_id":"...","session_id":"...","provider":"openai","model":"gpt-4o","timestamp":"2026-04-18T12:00:00Z"}
{"type":"tool_call_requested","run_id":"...","call":{"id":"c1","name":"read_file","arguments":{"path":"src/main.rs"}},"timestamp":"..."}
{"type":"run_completed","run_id":"...","timestamp":"..."}
```

The log is the source of truth for session state and supports full replay for debugging and audit.

---

## RPC API

The `crates/rpc-api` crate provides a fully working stdin/stdout JSONL RPC protocol for machine-to-machine agent control. External processes (editors, orchestrators, scripts) can drive the runtime by writing requests to stdin and reading responses from stdout.

### Starting the server

```rust
use rpc_api::stdio_server;

let server = stdio_server(); // reads stdin, writes stdout
server.run().await?;
```

`stdio_server()` is a convenience constructor. For custom transports, use `RpcServer::new(reader, writer)` with any `AsyncRead + AsyncWrite` pair.

### Protocol overview

- **Transport**: stdin/stdout, newline-delimited JSON (JSONL) — one complete JSON object per line
- **Client → server**: one `RpcRequest` per line
- **Server → client**: `RpcResponse` variants (see below)
- The server emits an `Ack` immediately on receiving each request, then follows with a `Success`, `StreamEvent` sequence, or `Error`

### Request format

```json
{"id":"<string>","method":{"<MethodName>":{/* params */}}}
```

The `id` field is a client-assigned string (UUID or any unique identifier) used to correlate responses.

### RPC methods

#### `SessionAttach` — attach to or create a session

```json
{"id":"req-1","method":{"SessionAttach":{"session_id":null}}}
```

Pass `"session_id": "<uuid>"` to attach to an existing session; `null` creates a new one.

**Success response:**
```json
{"kind":"Success","request_id":"req-1","data":{"session_id":"<uuid>","status":"Active","created_at":"2024-01-01T00:00:00Z","run_count":0,"label":null}}
```

#### `SessionDetach` — detach from a session

```json
{"id":"req-2","method":{"SessionDetach":{"session_id":"<uuid>"}}}
```

**Success response:**
```json
{"kind":"Success","request_id":"req-2","data":null}
```

#### `RunStart` — start a run with streaming output

```json
{"id":"req-3","method":{"RunStart":{"session_id":"<uuid>","prompt":"Hello, agent!","provider":null,"model":null}}}
```

The server emits an `Ack`, then a sequence of `StreamEvent` messages as the run progresses, and finally a `Success` with `RunInfo` when the run completes:

**Ack (immediate):**
```json
{"kind":"Ack","request_id":"req-3"}
```

**StreamEvent (repeated, one per chunk/event):**
```json
{"kind":"StreamEvent","event":{"seq":1,"timestamp":"2024-01-01T00:00:00Z","category":"run","session_id":"<uuid>","run_id":"<uuid>","event_type":"token_chunk","payload":{"delta":"Hello "}}}
```

**Success (terminal):**
```json
{"kind":"Success","request_id":"req-3","data":{"run_id":"<uuid>","session_id":"<uuid>","status":"Completed","created_at":"2024-01-01T00:00:00Z","completed_at":"2024-01-01T00:00:01Z"}}
```

#### `RunCancel` — cancel an in-progress run

```json
{"id":"req-4","method":{"RunCancel":{"session_id":"<uuid>","run_id":"<uuid>"}}}
```

`run_id` is optional; omitting it cancels the most recent active run in the session.

**Success response:**
```json
{"kind":"Success","request_id":"req-4","data":null}
```

#### `AuthStatus` — query auth state for a provider

```json
{"id":"req-5","method":{"AuthStatus":{"provider":"openai"}}}
```

**Success response:**
```json
{"kind":"Success","request_id":"req-5","data":{"provider_id":"openai","authenticated":true,"token_expires_at":"2024-06-01T00:00:00Z","flow":null}}
```

#### `Capabilities` — list supported methods and server capabilities

```json
{"id":"req-6","method":{"Capabilities":{"provider":"openai"}}}
```

**Success response:**
```json
{"kind":"Success","request_id":"req-6","data":{"protocol_version":"1.0","supported_methods":["session_attach","session_detach","run_start","run_cancel","auth_status","capabilities"],"streaming_supported":true,"tool_passthrough":false,"max_concurrent_runs":1}}
```

### Error response format

Errors include a structured `code` field for programmatic handling:

```json
{"kind":"Error","request_id":"req-x","code":"session_not_found","message":"session <id> not found"}
```

**Error codes:**

| Code | Meaning |
|------|---------|
| `parse_error` | Request line could not be parsed as valid JSON |
| `invalid_request` | JSON parsed but failed schema validation |
| `unknown_method` | Method name not recognised |
| `session_not_found` | Referenced session does not exist |
| `run_not_found` | Referenced run does not exist |
| `invalid_run_state` | Operation not valid for the run's current state |
| `auth_unavailable` | Auth info could not be retrieved for the provider |
| `capability_unavailable` | Provider does not support the requested capability |
| `internal_error` | Unexpected server-side failure |

### StreamEvent format

`StreamEvent` messages carry normalized `RpcEvent` objects pushed by the server outside of direct request/response flow:

| Field | Type | Description |
|-------|------|-------------|
| `seq` | `u64` | Monotonically increasing sequence number (per server instance) |
| `timestamp` | `string` | ISO 8601 UTC timestamp |
| `category` | `string` | One of: `session` · `run` · `tool` · `auth` · `system` |
| `session_id` | `string \| null` | Session this event belongs to, if applicable |
| `run_id` | `string \| null` | Run this event belongs to, if applicable |
| `event_type` | `string` | Normalized event tag (e.g. `token_chunk`, `tool_started`, `run_completed`) |
| `payload` | `object` | Safe external payload — internal fields stripped |

### Crate modules

| Module | Role |
|--------|------|
| `transport` | `LineReader` / `LineWriter` — async JSONL I/O with concurrent-write support |
| `protocol` | All protocol types: requests, responses, events, info structs, error codes |
| `server` | `RpcServer` main loop — parse-error recovery, broken-pipe handling, `ServerState` |
| `dispatch` | Per-method handler functions — session and run lifecycle, auth, capabilities |
| `normalize` | `normalize_event()` — exhaustive `AgentEvent` → `RpcEvent` mapping |
| `error` | `RpcError` — `SessionNotFound`, `RunNotFound`, `InvalidRunState`, `BrokenPipe`, `Internal` |

### Limitations

- **RunStart uses simulated streaming** — chunks are synthesised by the dispatch layer; real model-adapter streaming integration is deferred to Phase 10 (CLI).
- **AuthStatus is a stub** — returns placeholder data; real provider auth query deferred to Phase 10.
- **Embedding generation deferred** — `QdrantMemory` ANN search requires pre-computed embeddings; no embedding model is wired in yet.
- **Single run per stream** — the current server does not multiplex parallel runs over a single stdio channel.
- **No network transport** — only stdin/stdout; TLS / Unix socket variants deferred to Phase 12.

---

## CLI

The `rustpi` binary is a production-usable scriptable CLI built with `clap`. It wraps the full agent runtime and exposes every capability through a consistent command tree with human-readable streaming output and a stable JSON mode for automation.

### Build / install

```sh
# Build the binary into target/
cargo build -p cli

# Or install directly onto $PATH
cargo install --path crates/cli
```

### Command reference

| Command | Description |
|---|---|
| `rustpi run <PROMPT>` | Submit a prompt and stream the response |
| `rustpi run --file <PATH>` | Read prompt from a file |
| `rustpi session list` | List all active sessions |
| `rustpi session attach [--id <UUID>]` | Attach to an existing session |
| `rustpi session detach <UUID>` | Detach from a session |
| `rustpi session info <UUID>` | Show session metadata |
| `rustpi auth status [--provider <ID>]` | Check auth state for all or one provider |
| `rustpi auth login --provider <ID>` | Start an auth flow for a provider |
| `rustpi auth logout --provider <ID>` | Revoke stored credentials for a provider |
| `rustpi replay [--session-id <ID>]` | View the event timeline for a session |
| `rustpi diag` | Print a system diagnostics report (includes event log section) |
| `rustpi status` | Show configured providers, session store, and event log status |
| `rustpi sessions` | List past sessions from the persistent session store |

### Global flags

| Flag | Default | Description |
|---|---|---|
| `--output <FORMAT>` | `print` | Output format: `print` or `json` |
| `--provider <ID>` | config | Override the active provider |
| `--model <ID>` | config | Override the active model |
| `--session-id <UUID>` | — | Attach to an existing session by ID |
| `--non-interactive` | false | Fail immediately instead of prompting |
| `--config <PATH>` | auto | Override config file path |

### Usage examples

#### Submitting a prompt

```sh
# Positional argument
rustpi run "What is Rust?"

# Pipe from stdin
echo "Summarise the Rust ownership model" | rustpi run

# Read from a file
rustpi run --file task.md

# Override provider and model for one call
rustpi run --provider openai --model gpt-4o "Hello"
```

#### JSON output

```sh
# Machine-readable response
rustpi run --output json "Explain lifetimes"

# Streaming JSONL (one object per token chunk)
rustpi run --output json "Write a haiku"
```

**Success envelope:**
```json
{"ok": true, "data": {"response": "..."}}
```

**Error envelope:**
```json
{"ok": false, "error": {"code": "run_failed", "message": "Provider returned 429"}}
```

**Streaming (JSONL):**
```jsonl
{"event":"token_chunk","data":{"text":"Rust"}}
{"event":"token_chunk","data":{"text":" is"}}
{"event":"done","data":{}}
```

#### Piped I/O patterns

```sh
# Stdin pipe — no positional argument needed
cat notes.txt | rustpi run

# Combine with other tools
rustpi run --output json "List the top 5 risks" | jq '.data.response'
```

#### Session management

```sh
# List all active sessions
rustpi session list

# Start a run and continue it in the same session
ID=$(rustpi run --output json "Start a plan" | jq -r '.data.session_id')
rustpi run --session-id "$ID" "Add step 2"

# Inspect a session
rustpi session info "$ID"

# Detach when done
rustpi session detach "$ID"
```

#### Auth commands

```sh
# Check auth across all providers
rustpi auth status

# Check a single provider
rustpi auth status --provider openai

# Authenticate via GitHub device flow
rustpi auth login --provider github-copilot

# Revoke stored credentials
rustpi auth logout --provider openai
```

#### Diagnostics

```sh
# Human-readable report (includes event log section: recent failures + incomplete runs)
rustpi diag

# Machine-readable
rustpi diag --output json
```

#### Replay viewer

```sh
# Show the full event timeline (loads real event log if available; falls back to demo data)
rustpi replay

# Filter to a specific session
rustpi replay --session-id <UUID>

# Show only failed runs
rustpi replay --failures-only

# Show only the audit log (no token/response content)
rustpi replay --audit-only

# Machine-readable JSON timeline
rustpi replay --output json
```

### Output modes

| Mode | Description |
|---|---|
| `print` (default) | Human-readable; ANSI colour on TTY; token chunks streamed live to stdout |
| `json` | Single JSON object on completion; `{"ok":true,"data":{...}}` or `{"ok":false,"error":{...}}` |
| `json` (streaming) | JSONL — one `{"event":"token_chunk","data":{...}}` per chunk, followed by `{"event":"done","data":{}}` |

Streaming in `json` mode buffers to completion before emitting the final envelope. Use `print` for live streaming output.

### Non-interactive mode

Pass `--non-interactive` to prevent the CLI from prompting for input (browser opens, device code confirmations, missing config values). Any command that would otherwise prompt exits immediately with **code 3**.

This flag is intended for CI pipelines and scripted automation.

### Exit codes

| Code | Meaning |
|---|---|
| 0 | Success |
| 1 | General / runtime error |
| 2 | Invalid arguments |
| 3 | Auth required / auth failure / non-interactive prompt blocked |
| 4 | Session not found |
| 5 | Run execution failed |

### Limitations

- Sessions are in-memory only in this MVP; persistent session store integration in the CLI is deferred.
- Auth login flows are validated but actual OAuth browser-open is deferred to Phase 11.
- Streaming in `json` mode buffers to completion before output; use `print` for live output.
- TUI integration is deferred to Phase 11.

---

## TUI

The `rustpi-tui` binary is a full-screen terminal UI built on [Ratatui 0.29](https://github.com/ratatui-org/ratatui) + Crossterm. It connects to the shared `ServerState` event bus and renders live agent activity without blocking the runtime.

### Launch

```sh
cargo run -p tui        # development
cargo install --path crates/tui && rustpi-tui   # installed
```

### Layout

```
┌──────────────────────────────────────────┬──────────────────────────┐
│  Conversation [1]           (60% width)  │  Tool Activity [2] (40%) │
│                                          │                          │
│              (upper 65% of terminal)     │                          │
├──────────────────────────────────────────┴──────────────────────────┤
│  [Session: xxxxxxxx] [Run: idle]  …status…  | Keys: 1-6 … ? help   │
├─────────────────────────────────────────────────────────────────────┤
│  Input bar: > _                                                     │
├───────────────────┬───────────────────┬────────────┬───────────────┤
│  Sessions [4]     │  Context [3]      │  Auth [5]  │  Logs [6]     │
│  (25% each)                                                        │
└───────────────────┴───────────────────┴────────────┴───────────────┘
```

### Panes

| Pane | Key | Description |
|------|-----|-------------|
| Conversation | `1` | Live chat log with role-colored lines, streaming token cursor (▌), and inline approval prompts |
| Tool Activity | `2` | Tool lifecycle events — started / stdout / stderr / done / failed / cancelled; most-recent 20 entries |
| Context | `3` | File count and token budget info populated by `ContextBuilt` events |
| Sessions | `4` | Session list with active-session marker (●) and scrollable cursor |
| Auth | `5` | Per-provider auth status — color-coded: ● authenticated · ◐ pending · ✗ expired/failed · ○ unauthenticated |
| Logs | `6` | Runtime log ring buffer (capped at 500 entries), color-coded by level |

### Keyboard shortcuts

| Key | Action |
|-----|--------|
| `1`–`6` | Focus pane |
| `q` / `Ctrl+C` | Quit |
| `j` / `↓` | Scroll focused pane down |
| `k` / `↑` | Scroll focused pane up |
| `PgDn` | Page scroll down (10 lines) |
| `PgUp` | Page scroll up (10 lines) |
| `Enter` | Submit typed prompt |
| `Ctrl+I` | Request interrupt on active run |
| `y` | Approve pending action |
| `n` | Deny pending action |
| `?` | Show key reference in status bar |

### Approval workflow

When the agent requests a tool call whose name contains `write`, `exec`, `shell`, or `delete`, the Conversation pane renders a highlighted inline prompt:

```
⚠ Approve [<tool_name>]? (y=yes / n=no): Tool: <name> args: <arguments>
```

Press `y` to approve or `n` to deny. The decision is reflected immediately in the status bar (`"Action approved"` / `"Action denied"`) and the `pending_approval` state is cleared.

### Interrupt workflow

While a run is active (`[Run: active]` in the status bar), press `Ctrl+I` to request an interrupt. A log entry is written (`"Interrupt requested for run <id>"`) and the status bar updates to `"Interrupt requested"`. Full run cancellation wiring is deferred to Phase 12.

### Limitations

- Sessions are in-memory — not persisted between TUI restarts.
- Prompt submission appends a User message locally; real model streaming integration is deferred to Phase 12.
- Auth login flow cannot be initiated from the TUI yet (shows status only).
- Context pane populates only when `ContextBuilt` events arrive from a connected runtime.
- Interrupt is advisory — `Ctrl+I` logs the intent but does not yet send a cancellation token to the executor.
- Minimum recommended terminal size: **80×24**.

---

## Observability / Telemetry

The `crates/observability` crate provides real-time metrics collection by subscribing to the `EventBus` broadcast channel. No additional instrumentation is required — every `AgentEvent` the runtime already emits is automatically captured.

### What is tracked

| Metric | Source | Details |
|--------|--------|---------|
| Runs started / failed / cancelled | `RunStarted`, `RunFailed`, `RunCancelled` events | Per provider |
| Provider latency | `RunStarted` → `RunCompleted` wall-clock delta | Per model provider |
| Provider error rate | failed / (started) | Per provider |
| Token usage per run | `TokenChunk.delta` length accumulation | Estimated; per run ID |
| Tool failures | `ToolExecutionFailed` events | Cumulative counter |
| Tool cancellations | `ToolExecutionCancelled` events | Cumulative counter |

### Components

| Type | Role |
|------|------|
| `TelemetryCollector` | Subscribes to the `EventBus` broadcast; dispatches events to the individual trackers |
| `ProviderMetrics` | Per-provider counters: runs started, failed, cancelled; latency histogram; error rate |
| `TokenUsageTracker` | Accumulates estimated token deltas per `RunId` |
| `ToolMetrics` | Failure and cancellation counters across all tools |
| `TelemetrySummary` | Point-in-time snapshot of all metrics; serialises to JSON |
| `ObservabilityError` | Error type for the crate |

### Getting a snapshot

```rust
use observability::TelemetryCollector;

// Wire up collector on startup (give it the broadcast receiver from EventBus)
let collector = TelemetryCollector::new(event_rx);
tokio::spawn(collector.run());

// Anywhere in the runtime — take a snapshot
let summary: TelemetrySummary = collector.snapshot();
let json = serde_json::to_string_pretty(&summary)?;
println!("{json}");
```

```json
{
  "providers": {
    "openai": {
      "runs_started": 42,
      "runs_failed": 1,
      "runs_cancelled": 0,
      "error_rate": 0.024,
      "mean_latency_ms": 1340
    }
  },
  "token_usage": {
    "<run-uuid>": 812
  },
  "tool_failures": 2,
  "tool_cancellations": 0
}
```

### Limitations

- Token counts are **estimated** from `TokenChunk.delta` byte length (same `~4 chars/token` heuristic used elsewhere); byte-exact tokenizer integration is deferred.
- Metrics are **in-memory only** — they reset on process restart. Persistent metrics storage (e.g. Prometheus exporter, InfluxDB sink) is deferred to a future phase.
- No Prometheus `/metrics` endpoint yet.

---

## Replay Viewer

The `rustpi replay` command loads the on-disk event log and renders a human-readable timeline of all agent activity, for debugging and audit.

### Usage

```sh
# Full timeline for the default session
rustpi replay

# Scoped to one session
rustpi replay --session-id <UUID>

# Show only runs that ended with a failure
rustpi replay --failures-only

# Audit-only mode — hides token content; shows only structural events
rustpi replay --audit-only

# Machine-readable JSON array of TimelineEntry objects
rustpi replay --output json
```

### TimelineEntry fields (JSON mode)

| Field | Type | Description |
|-------|------|-------------|
| `timestamp` | ISO 8601 string | When the event occurred |
| `run_id` | string | Run this event belongs to |
| `kind` | string | Event type tag (e.g. `RunStarted`, `ToolFailed`) |
| `detail` | string | Human-readable summary of the event |
| `is_failure` | bool | `true` for `RunFailed` / `ToolExecutionFailed` events |

### What `diag` now shows

`rustpi diag` now includes an **Event Log** section that reports:
- Recent failures — runs that ended with a `RunFailed` event, with reason strings
- Incomplete runs — runs that have a `RunStarted` with no terminal event (`RunCompleted` / `RunFailed` / `RunCancelled`)

### Fault-tolerant log loading

`EventLogReader::from_file_tolerant()` skips corrupt or unreadable lines rather than failing the entire load. Each skipped line is counted and surfaced in the reader's diagnostics, so you can replay partial logs after a crash.

---

## Crash Recovery

`crates/session-store` now includes a startup crash recovery subsystem that reconciles in-progress or pending runs left over from an unclean shutdown.

### `run_startup_recovery()`

Call this once at process startup, before accepting new work:

```rust
use session_store::recovery::run_startup_recovery;

run_startup_recovery(&store, &SafeResumePolicy::default()).await?;
```

It scans all runs in `Running` or `Pending` state, applies `SafeResumePolicy`, and updates their status accordingly.

### `SafeResumePolicy` behaviour

| Condition | Action | `ReconciledStatus` |
|-----------|--------|--------------------|
| No tool activity (conversational run) | Auto-resume | `Resumable` |
| Tool side-effects detected | Halt; operator must decide | `RequiresApproval` |
| Run started > 24 hours ago | Cancel automatically | `Cancelled` |
| Run already in a terminal state | Skip silently | `AlreadyTerminal` |
| Run explicitly cancelled before crash | Preserve as cancelled | `Cancelled` |

### `ReconciledStatus` variants

| Variant | Meaning |
|---------|---------|
| `Resumable` | Safe to restart the run automatically |
| `RequiresApproval` | Run had tool side-effects; operator must approve before resume |
| `Cancelled` | Run was cancelled (old age policy or prior cancellation) |
| `Failed` | Run ended in failure before the crash |
| `AlreadyTerminal` | Run was already completed/failed/cancelled; no action needed |

### `RecoveryScanner`

`RecoveryScanner` is the lower-level API. It returns a `Vec<ReconcileOutcome>` for all scanned runs so callers can inspect or log each decision:

```rust
let scanner = RecoveryScanner::new(&store, policy);
let outcomes = scanner.scan().await?;
for outcome in &outcomes {
    println!("{}: {:?}", outcome.run_id, outcome.status);
}
```

### `RecoveryRunRecord`

A lightweight projection of a run used during scanning — contains `run_id`, `session_id`, `status`, `started_at`, and a `has_tool_activity` flag derived from the event log.

---

## Security Model

rustpi implements a layered security model across authentication, tool execution, file access, secret handling, and audit logging.

### Security controls summary

| Control | What it protects | Where enforced | Configurable |
|---------|-----------------|----------------|--------------|
| AES-256-GCM token encryption | Auth token theft | `auth-core/EncryptedFileTokenStore` | Key path via config |
| Secrets redaction | Secret leakage in logs/events | `agent-core/Redactor`, subprocess pipeline | Pattern list |
| Path safety policy | Path traversal / unsafe file access | `tool-runtime/PathSafetyPolicy` | Allowed roots, deny list |
| Command policy | Dangerous shell commands | `tool-runtime/CommandPolicy` | Built-in + custom rules |
| Overwrite policy | Unsafe file overwrites | `tool-runtime/OverwritePolicy` | Allow / DenyExisting / RequireConfirmation |
| Tool approval hooks | High/Critical tool execution | `tool-runtime/ApprovalHook` | DenyAbove threshold, AllowList |
| Policy engine | Tool, file, provider, auth access | `policy-engine/PolicyEngine` | Glob rules, default policy |
| Security audit log | Traceability of denials/approvals | `AgentEvent` variants, `AuditSink` | Via event bus |
| Vault mutability | Memory vault doc integrity | `memory-sync/VaultAccessor` | Per-doc mutability |

### Secure Token Storage

Tokens are persisted encrypted at rest using AES-256-GCM in `~/.config/rustpi/tokens.enc`. The encryption key is stored at a configurable path (defaults to an adjacent file in the same directory).

```rust
// In config:
// [auth]
// token_store_path = "~/.config/rustpi/tokens.enc"
// key_path = "~/.config/rustpi/tokens.key"  // optional override
```

**Limitation:** Key and ciphertext live in the same directory by default. Platform keyring integration (macOS Keychain, Secret Service, Windows DPAPI) is deferred.

### Secrets Redaction

`Redactor` in `crates/agent-core/src/redaction.rs` applies five compiled regex patterns:

- Bearer tokens in `Authorization:` headers
- API key prefixes: `sk-`, `ghp_`, `gho_`, `ghu_`, `ghs_`, `xoxb-`, `AKIA…`
- Raw `Authorization:` header lines
- `token=`, `secret=`, `password=` key-value assignments
- Long base64-encoded credential blobs

All matches are replaced with `[REDACTED]`. Integrated automatically into the `ShellTool` subprocess pipeline — every `ToolStdout` / `ToolStderr` line is scrubbed before reaching the event bus.

To extend with custom patterns:
```rust
use agent_core::redaction::Redactor;

// Redactor::new() accepts additional regex patterns
let redactor = Redactor::with_extra_patterns(&["my-secret-prefix-[A-Za-z0-9]+"])?;
```

### Command Policy

`CommandPolicy` in `crates/tool-runtime/src/command_policy.rs` uses first-match-wins rule evaluation. `ShellTool` loads `CommandPolicy::with_defaults()` automatically, which denies:

- `rm -rf /` and `rm -rf /*`
- `dd if=/dev/` (raw disk overwrite)
- `mkfs*` (filesystem format commands)
- `:(){ :|:& };:` (fork bomb)
- `chmod -R 777 /` and `chmod 777 /`
- `/dev/sda` writes and `shred /dev/`

To customize:
```rust
use tool_runtime::command_policy::{CommandPolicy, CommandRule, CommandPattern, CommandAction};

let policy = CommandPolicy::with_defaults()
    .add_rule(CommandRule {
        name: "no-curl-exfil".into(),
        pattern: CommandPattern::Contains("curl http://evil.example".into()),
        action: CommandAction::Deny("exfiltration target blocked".into()),
    });

let shell = ShellTool::new(path_policy).with_policy(policy);
```

### File Overwrite Safeguards

`OverwritePolicy` in `crates/tool-runtime/src/overwrite_policy.rs` controls how `WriteFileTool` and `EditFileTool` behave when the target file already exists:

| Variant | Behaviour |
|---------|-----------|
| `Allow` | Default; no additional checks |
| `DenyExisting` | Rejects writes to existing paths; `ToolError::OverwriteDenied` |
| `RequireConfirmation` | Requires `overwrite: true` in tool arguments; `ToolError::OverwriteNotConfirmed` |

```rust
use tool_runtime::{
    overwrite_policy::OverwritePolicy,
    tools::file::{WriteFileTool, EditFileTool},
};

let write = WriteFileTool::new_with_policy(path_policy.clone(), OverwritePolicy::RequireConfirmation);
let edit  = EditFileTool::new_with_policy(path_policy.clone(), OverwritePolicy::DenyExisting);
```

### Security Audit Logging

Six new `AgentEvent` variants carry structured denial/approval records emitted to the event bus:

| Variant | When emitted |
|---------|--------------|
| `ApprovalDenied` | `ApprovalHook` rejects a tool call |
| `ApprovalGranted` | `ApprovalHook` approves a tool call |
| `CommandDenied` | `CommandPolicy` blocks a shell command (preview truncated to 100 chars) |
| `PathDenied` | `PathSafetyPolicy` rejects a path |
| `OverwriteBlocked` | `OverwritePolicy` blocks a file write |
| `PolicyDenied` | `PolicyEngine` denies a request (domain + rule context) |

Wire `AuditSink` into the tool runner to emit these events:

```rust
use tool_runtime::audit::AuditSink;

let (event_tx, event_rx) = tokio::sync::broadcast::channel(128);
let audit = AuditSink::new(event_tx.clone());

let runner = ToolRunner::new(registry, Duration::from_secs(30))
    .with_approval(approval_hook)
    .with_event_tx(event_tx)
    .with_audit_sink(audit);
```

All security events are serialised to the JSONL event log for later replay and audit via `rustpi replay --audit-only`.

### Threat Model Summary

#### Attack surfaces and mitigations

| Attack surface | Threat | Mitigations |
|----------------|--------|-------------|
| Subprocess output | Secret leakage into logs or event bus | `Redactor` in pipeline (Phase 13) |
| Shell tool | Dangerous commands — rm, disk overwrite, fork bomb | `CommandPolicy::with_defaults()` (Phase 13); `ApprovalHook` Critical sensitivity (Phase 5) |
| File tools | Unintended overwrite of existing files | `OverwritePolicy` (Phase 13); `PathSafetyPolicy` traversal guard (Phase 5) |
| Path arguments | Directory traversal outside workspace | `PathSafetyPolicy` allowed roots + deny list (Phase 5) |
| Token storage | Auth token theft from disk | AES-256-GCM `EncryptedFileTokenStore` (Phase 3) |
| Tool approval gaps | High/Critical tools running without oversight | `AuditSink` events (Phase 13); `DenyAbove` / `AllowList` hooks (Phase 5) |
| Policy bypass | Tool/file/provider access outside rules | `AuditSink` `PolicyDenied` event (Phase 13); `PolicyEngine` glob rules (Phase 2) |
| Vault mutation | Runtime corrupting human-authored vault sections | `VaultAccessor` per-doc mutability modes (Phase 8) |

#### Residual risks (deferred)

- Platform keyring integration — key and ciphertext co-located on disk
- Command policy uses substring matching, not shell AST parsing
- Regex redaction may miss obfuscated or split secrets
- No network egress controls on provider adapters or subprocess
- No rate limiting on RPC/CLI surfaces
- No Prometheus/OTEL security metrics exporter

### Security Testing

```bash
# Run all security-related tests
cargo test --workspace

# Targeted security crate tests
cargo test -p agent-core        # Redactor unit tests
cargo test -p tool-runtime      # CommandPolicy, OverwritePolicy, AuditSink, PathSafetyPolicy tests
cargo test -p auth-core         # EncryptedFileTokenStore tests
cargo test -p policy-engine     # PolicyEngine rule evaluation tests
```

---



| Phase | Title | Status |
|---|---|---|
| 0 | Foundation and technical design | ✅ Complete |
| 1 | Core runtime skeleton | ✅ Complete |
| 2 | Config, policy, and event logging | ✅ Complete |
| 3 | Model adapter abstraction and auth core | ✅ Complete |
| 4 | First provider integrations | ✅ Complete |
| 5 | Tool runtime MVP | ✅ Complete |
| 6 | Context engine MVP | ✅ Complete |
| 7 | Session stores and durable memory backends | ✅ Complete |
| 8 | Obsidian vault memory and personality system | ✅ Complete |
| 9 | RPC API | ✅ Complete |
| 10 | CLI | ✅ Complete |
| 11 | Ratatui TUI | ✅ Complete |
| 12 | Observability, replay, and reliability hardening | ✅ Complete |
| 13 | Security hardening | ✅ Complete |
| 14 | Full-system testing and parity validation | ✅ Complete |

See [`project.md`](./project.md) for detailed checklists and exit criteria per phase.

---

## Repository layout

```
rustpi/
├── crates/
│   ├── agent-core/       # Shared types and traits
│   ├── auth-core/        # Auth flows and token storage
│   ├── cli/              # rustpi binary
│   ├── config-core/      # Layered config system
│   ├── context-engine/   # Context window assembly
│   ├── event-log/        # Append-only JSONL event log + replay
│   ├── memory-sync/      # Obsidian vault + vector memory
│   ├── model-adapters/   # Provider abstraction and registry
│   ├── observability/    # TelemetryCollector, metrics, TelemetrySummary
│   ├── policy-engine/    # Allow/deny/approval rules
│   ├── rpc-api/          # JSONL RPC protocol
│   ├── session-store/    # Session persistence + crash recovery
│   ├── tool-runtime/     # Tool execution engine
│   └── tui/              # rustpi-tui binary
├── docs/
│   ├── adr/              # Architecture decision records
│   └── architecture.md   # Full crate reference and dependency diagram
├── .github/
│   └── workflows/ci.yml  # fmt · clippy · build · test
├── CONTRIBUTING.md
└── project.md            # Phased development plan
```

---

## Provider Configuration

### OpenAI-compatible
```toml
[providers.openai]
base_url = "https://api.openai.com/v1"
api_key = "sk-..."   # or set OPENAI_API_KEY env var
```

### llama.cpp (local)
```toml
[providers.llamacpp]
base_url = "http://localhost:8080/v1"
# No authentication required
```

### vLLM (local)
```toml
[providers.vllm]
base_url = "http://localhost:8000/v1"
# api_key is optional — required only if vLLM was started with --api-key
```

### GitHub Copilot
Requires a GitHub account with an active Copilot subscription.
Authentication uses the GitHub device flow:
```bash
rustpi auth login --provider copilot
# Follow the device code instructions to authenticate
```

---

---

## Testing

### Test strategy

rustpi's test suite is organised in five layers:

1. **Unit tests** (`#[cfg(test)]` modules inline in each crate's `src/`) — validate individual functions, type invariants, and error paths in isolation.
2. **Integration tests** (`crates/*/tests/`) — exercise public APIs across crate boundaries, including backend parity macros that run the same assertions against SQLite and sled.
3. **Provider matrix tests** (`crates/model-adapters/tests/`) — wiremock mock HTTP servers validate request formatting, streaming, error normalisation, and 429/503 handling across OpenAI-compatible, llama.cpp, and vLLM adapters without requiring live API credentials.
4. **Backend matrix tests** (`crates/session-store/tests/`, `crates/memory-sync/tests/`) — SQLite and sled backends run offline; PostgreSQL and Qdrant tests are marked `#[ignore]` and require live services.
5. **Failure mode tests** — deliberate error-path coverage: stream connection drop, provider timeout, shell timeout, nonexistent command, corrupted JSONL, invalid UTF-8, partial replay log.

### How to run

| Category | Command | Notes |
|---|---|---|
| All tests | `cargo test --workspace` | Requires no external services |
| Unit only | `cargo test --workspace --lib` | |
| Provider matrix | `cargo test -p model-adapters` | Uses wiremock mock servers |
| Backend matrix | `cargo test -p session-store -p memory-sync` | SQLite/sled offline; PG/Qdrant `#[ignore]` |
| Lifecycle integration | `cargo test -p agent-core` | Runs on a tmpdir; no external deps |
| Failure modes | `cargo test -p tool-runtime -p event-log -p auth-core` | |
| PostgreSQL tests | `DATABASE_URL=postgres://... cargo test -p session-store -- --include-ignored postgres` | Requires live PG |
| Qdrant tests | `cargo test -p memory-sync -- --include-ignored qdrant` | Requires live Qdrant |

Current workspace total: **~601 test functions**, 0 failures on `cargo test --workspace`.

### Manual test checklists

For features that cannot be fully covered by automated tests:

- [`docs/testing/cli-tui-checklist.md`](./docs/testing/cli-tui-checklist.md) — 85 items covering CLI commands, TUI interaction, streaming output, and keyboard shortcuts
- [`docs/testing/memory-sync-checklist.md`](./docs/testing/memory-sync-checklist.md) — 37 items covering vault sync, conflict resolution, and personality loading
- [`docs/testing/release-readiness-checklist.md`](./docs/testing/release-readiness-checklist.md) — 40 items across 8 sections for pre-release validation

### Known limitations

The following are **not** covered by the automated test suite:

- **Live OAuth browser flow** — `AuthFlow::OAuthBrowser` opens the system browser; cannot be exercised in CI.
- **Live provider API calls** — all provider tests use wiremock stubs; no real OpenAI/Copilot/Gemini calls are made.
- **TUI terminal interaction** — Ratatui rendering and keyboard input are not tested programmatically; covered by the CLI/TUI checklist.
- **RPC model passthrough** — `run_start` in the RPC server currently drives a simulated token stream, not a real model adapter; end-to-end model-via-RPC is deferred.
- **Gemini adapter** — declared in the provider list but not yet implemented; only OpenAI-compatible, llama.cpp, vLLM, and Copilot adapters exist.
- **Embedding-based ANN search** — `QdrantMemory::search_similar()` requires pre-computed embeddings; no embedding model is wired in, so semantic search falls back to scroll + keyword filtering.

---

## Running Tests

```bash
# All tests
cargo test --workspace

# Provider adapter tests only (includes mock HTTP server tests)
cargo test -p model-adapters

# Specific adapter tests
cargo test -p model-adapters adapters::openai
cargo test -p model-adapters adapters::copilot

# Observability tests (11 tests)
cargo test -p observability

# Event-log replay tests (33 tests total)
cargo test -p event-log

# CLI replay + diag tests (27 tests total)
cargo test -p cli

# Session-store recovery tests (10 tests)
cargo test -p session-store
```

Note: Provider integration tests use `wiremock` mock servers and do not require live API credentials.

---

## ADRs

Architecture decision records live in [`docs/adr/`](./docs/adr/):

- [ADR-0001](./docs/adr/0001-runtime-event-model.md) — Runtime event model
- [ADR-0002](./docs/adr/0002-provider-abstraction.md) — Provider abstraction
- [ADR-0003](./docs/adr/0003-tool-execution-model.md) — Tool execution model
- [ADR-0004](./docs/adr/0004-memory-layering.md) — Memory layering
- [ADR-0005](./docs/adr/0005-obsidian-sync-model.md) — Obsidian sync model
- [ADR-0006](./docs/adr/0006-auth-token-handling.md) — Auth and token handling

---

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md) for build commands, coding conventions, error handling rules, and the testing strategy.

