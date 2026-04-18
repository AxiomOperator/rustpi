# rustpi

A native Rust AI agent platform with multi-provider model access, durable sessions, Obsidian-backed local-first memory, and a rich terminal UI.

**Status: Phases 0–8 complete — vault memory and personality system live**

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

rustpi is a Cargo workspace of 13 focused library crates and two binary entry points. The dependency graph flows strictly upward — primitive types live at the bottom, binaries at the top, with no circular dependencies.

```
cli / tui  (binaries)
    │
rpc-api ──────────────────────────────────┐
    │                                     │
model-adapters  tool-runtime  context-engine  memory-sync
    │               │
auth-core       policy-engine
    │               │
session-store   event-log    config-core
    └───────────────┴────────────┘
                    │
              agent-core  (shared types, AgentEvent, core traits)
```

### Crate reference

| Crate | Role | Status |
|---|---|---|
| `agent-core` | Shared types, `AgentEvent` hierarchy, core traits | ✅ Phase 1 |
| `config-core` | Layered config (global/user/project/runtime), TOML loading, merge rules | ✅ Phase 2 |
| `policy-engine` | Allow/deny/approval rule evaluation with glob matching | ✅ Phase 2 |
| `event-log` | Append-only JSONL event log, replay reader, audit records | ✅ Phase 2 |
| `auth-core` | OAuth browser flow, RFC 8628 device flow, API key path, AES-256-GCM token store, refresh | ✅ Phase 3 |
| `model-adapters` | `ModelProvider` trait, provider registry; OpenAI-compatible, llama.cpp, vLLM, and GitHub Copilot adapters | ✅ Phase 4 |
| `tool-runtime` | Tool trait, registry, subprocess runner with timeout | ✅ Phase 5 |
| `context-engine` | Context window assembly, ignore rules, token budgeting | ✅ Phase 6 |
| `session-store` | Durable session persistence (SQLite / sled / PostgreSQL); 4 store traits + config-driven factory | ✅ Phase 7 |
| `memory-sync` | Qdrant semantic memory (`QdrantMemory`); Obsidian vault reader/writer, canonical docs, personality loader, sync engine | ✅ Phase 8 |
| `rpc-api` | JSONL RPC protocol types and codec | 🔧 Phase 9 |
| `cli` | Scriptable CLI binary (`rustpi`) | 🔧 Phase 10 |
| `tui` | Ratatui full-screen TUI binary (`rustpi-tui`) | 🔧 Phase 11 |

---

## Quick start

```sh
git clone <repo>
cd rustpi
cargo build --workspace
cargo test --workspace
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

## Development status

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
| 9 | RPC API | 🔲 Planned |
| 10 | CLI | 🔲 Planned |
| 11 | Ratatui TUI | 🔲 Planned |
| 12 | Observability, replay, and reliability hardening | 🔲 Planned |
| 13 | Security hardening | 🔲 Planned |
| 14 | Full-system testing and parity validation | 🔲 Planned |

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
│   ├── event-log/        # Append-only JSONL event log
│   ├── memory-sync/      # Obsidian vault + vector memory
│   ├── model-adapters/   # Provider abstraction and registry
│   ├── policy-engine/    # Allow/deny/approval rules
│   ├── rpc-api/          # JSONL RPC protocol
│   ├── session-store/    # Session persistence
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

## Running Tests

```bash
# All tests
cargo test --workspace

# Provider adapter tests only (includes mock HTTP server tests)
cargo test -p model-adapters

# Specific adapter tests
cargo test -p model-adapters adapters::openai
cargo test -p model-adapters adapters::copilot
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

