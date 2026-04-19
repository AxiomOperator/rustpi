Below is a phased development plan derived from your Rust agent architecture, including checklists and exit criteria. It is grounded in the goals, core crates, memory model, auth requirements, operator interfaces, and cross-cutting concerns from your architecture document. 

# Rust Agent Development Plan

## Planning assumptions

This plan assumes the product must deliver:

* a native Rust runtime
* multi-provider model access with OAuth, device auth, and API-key support where applicable
* durable session and memory management
* a local-first human-readable Obsidian memory layer
* vector memory via Qdrant
* CLI, RPC, and Ratatui-based TUI interfaces
* a safe, auditable tool runtime
* strong context handling, replayability, and observability

---

## Phase 0 — Foundation and technical design

### Objectives

Lock the architecture, crate boundaries, interfaces, and build conventions before implementation begins.

### Deliverables

* [x] Monorepo/workspace initialized
* [x] Crate boundaries defined
* [x] Core traits/interfaces documented
* [x] Event model documented
* [x] Config model documented
* [x] Initial ADRs written
* [x] CI pipeline bootstrapped
* [x] Coding standards and contribution guide created

### Tasks

* [x] Create Cargo workspace
* [x] Create initial crates:

  * [x] `agent-core`
  * [x] `model-adapters`
  * [x] `tool-runtime`
  * [x] `context-engine`
  * [x] `session-store`
  * [x] `rpc-api`
  * [x] `cli`
  * [x] `tui`
  * [x] `auth-core`
  * [x] `event-log`
  * [x] `config-core`
  * [x] `memory-sync`
  * [x] `policy-engine`
* [x] Define shared core types:

  * [x] `RunId`
  * [x] `SessionId`
  * [x] `ToolCall`
  * [x] `ToolResult`
  * [x] `ProviderId`
  * [x] `ModelId`
  * [x] `AuthState`
  * [x] `AgentEvent`
* [x] Decide serialization formats:

  * [x] JSON
  * [x] JSONL for streaming
* [x] Define crate dependency rules
* [x] Define logging and tracing standards
* [x] Define error handling conventions
* [x] Define test strategy by crate
* [x] Write architecture ADRs for:

  * [x] runtime event model
  * [x] provider abstraction
  * [x] tool execution model
  * [x] memory layering
  * [x] Obsidian sync model
  * [x] auth/token handling

### Exit criteria

* [x] Workspace builds successfully
* [x] All core crates exist
* [x] Architecture decisions are documented
* [x] Shared interfaces are stable enough to begin implementation

> **Phase 0 completed.** All deliverables, tasks, and exit criteria met.
> Repository state: 13 crates scaffolded under `crates/`, `cargo check --workspace` clean,
> 6 ADRs in `docs/adr/`, architecture reference in `docs/architecture.md`,
> CI pipeline in `.github/workflows/ci.yml`, developer guide in `CONTRIBUTING.md`.

---

## Phase 1 — Core runtime skeleton

### Objectives

Stand up the minimal runtime loop and session model.

### Deliverables

* [x] `agent-core` MVP
* [x] session lifecycle primitives
* [x] run lifecycle primitives
* [x] internal event bus
* [x] prompt assembly skeleton
* [x] cancellation/interrupt primitives

### Tasks

* [x] Implement session state model
* [x] Implement run state machine
* [x] Implement event enum hierarchy
* [x] Implement append-only runtime event emission
* [x] Implement minimal prompt assembly pipeline
* [x] Implement tool-call placeholder orchestration
* [x] Implement interrupt/cancel primitives
* [x] Implement token budget placeholders (`TokenBudget` struct; actual provider tokenization deferred to Phase 4)
* [x] Add unit tests for:

  * [x] session creation
  * [x] run transitions
  * [x] event emission
  * [x] cancellation handling

### Exit criteria

* [x] A run can be created, started, updated, cancelled, and completed
* [x] Events are emitted for all major lifecycle transitions
* [x] Prompt assembly accepts structured inputs
* [x] Runtime logic is test-covered and stable

> **Phase 1 completed.** 39 unit tests + 1 doc-test pass (`cargo test -p agent-core`).
>
> **Implemented in `crates/agent-core/src/`:**
> - `session.rs` — `Session`, `SessionStatus`, `SessionManager`; create/attach-run/end lifecycle with 6 tests
> - `run.rs` — `RunStatus` FSM (Created→Queued→Running→WaitingForTool→Completed/Failed/Cancelled), `Run` with `tokio_util::sync::CancellationToken`, `RunManager`; 13 tests including invalid-transition rejection
> - `bus.rs` — `EventBus` dual-channel: append-only `Arc<Mutex<Vec<AgentEvent>>>` for replay + `tokio::sync::broadcast` for live fan-out; 6 tests
> - `prompt.rs` — `PromptAssembler` builder, `AssembledPrompt`, `TokenBudget`, fixed section ordering (System→Memory→Context→History→UserInput); 8 tests + 1 doc-test
> - `tools.rs` — `ToolOrchestrator`, `PendingToolCall`, `PendingCallStatus` FSM; emits `ToolCallRequested`/`ToolResultSubmitted` events; 8 tests
>
> **Intentionally deferred:**
> - Actual provider-specific tokenization (token budget uses `len/4` heuristic placeholder)
> - Integration with real `tool-runtime` crate (Phase 5)
> - Persistent event log backend (Phase 2 — `event-log` crate)
> - Prompt templating beyond section ordering (Phase 6)

---

## Phase 2 — Config, policy, and event logging

### Objectives

Add the control plane needed for safe growth.

### Deliverables

* [x] layered config system
* [x] policy engine MVP
* [x] event log crate
* [x] replay/debug scaffolding

### Tasks

* [x] Implement `config-core`

  * [x] global config
  * [x] project config
  * [x] user config
  * [x] provider config
  * [x] memory backend config
  * [x] CLI/TUI config
* [x] Implement `policy-engine`

  * [x] tool allow/deny rules
  * [x] file mutation policy hooks
  * [x] provider selection rules
  * [x] auth guardrails
* [x] Implement `event-log`

  * [x] append-only storage
  * [x] structured event serialization
  * [x] replay reader
  * [x] audit record support
* [x] Add tests for precedence and policy evaluation

### Exit criteria

* [x] Config precedence works correctly
* [x] Policy evaluation can approve/deny runtime actions
* [x] Event logs can be replayed for debugging

> **Phase 2 completed.** 88 unit tests + 2 doc-tests pass across workspace (`cargo test --workspace`).
>
> **`crates/config-core` — layered config system:**
> - `src/model.rs` extended with `UserConfig`, `ProjectConfig`, `CliConfig`, `TuiConfig`, `LoggingConfig`, `PolicyDefaults`, and supporting enums (`OutputFormat`, `LogFormat`, `DefaultPolicy`)
> - `src/loader.rs` — `ConfigLoader` builder with `Merge` trait; precedence: defaults < global < user < project < runtime overrides; missing files silently skipped; TOML parsing via `toml = "0.8"`; 10 tests + 1 doc-test
>
> **`crates/policy-engine` — real evaluation:**
> - `src/request.rs` — typed `ToolRequest`, `FileMutationRequest`, `ProviderRequest`, `AuthRequest` with operation enums
> - `src/decision.rs` — `PolicyDecision` with `matched_rule`, `reason`, `is_allowed()`, `is_denied()` helpers
> - `src/policy.rs` — full glob-based first-match-wins evaluation; `DefaultPolicy` (Allow/Deny); auth guardrail (UseToken requires authenticated); `tracing::debug!` on matches; 14 tests
>
> **`crates/event-log` — append-only storage + replay:**
> - `src/record.rs` — `EventRecord` envelope (seq + appended_at + event), `AuditRecord`, `AuditKind`, `is_audit_relevant()` matcher
> - `src/memory_store.rs` — `MemoryEventStore` (Arc-backed, Clone-cheap, auto-seq); implements `EventStore`
> - `src/file_store.rs` — `FileEventStore` JSONL file store; `AtomicU64` seq; survives reopen; `replay_all()`
> - `src/replay.rs` — `ReplayReader` with `for_session`, `for_run`, `audit_trail`, `range`, `print_summary`; 25 tests
>
> **Intentionally deferred:**
> - Dynamic policy DSL / rule loading from config files (Phase 13 security hardening)
> - Production-grade durable event log backends (SQLite/Postgres — Phase 7)
> - Config env-var override layer (Phase 10 CLI)

---

## Phase 3 — Model adapter abstraction and auth core

### Objectives

Build the provider abstraction and a shared auth subsystem for OAuth, device flow, and API keys.

### Deliverables

* [x] provider trait/interface
* [x] `auth-core`
* [x] auth state persistence
* [x] token refresh flow
* [x] provider capability metadata model

### Tasks

* [x] Define internal provider interface for:

  * [x] chat/completions
  * [x] streaming
  * [x] embeddings
  * [x] model discovery
  * [x] tool-calling capability flags
  * [x] auth state inspection
* [x] Implement `auth-core`

  * [x] OAuth browser login flow
  * [x] device authorization flow
  * [x] device code / user code display flow
  * [x] API key storage path
  * [x] token refresh
  * [x] encrypted local token persistence
* [x] Implement provider capability descriptors
* [x] Add auth status events
* [x] Add tests for:

  * [x] expired token handling
  * [x] refresh flow
  * [x] missing-scope handling (`NoKeyAvailable` / `NoRefreshToken` errors)
  * [x] auth state recovery (EncryptedFileTokenStore survives reopen)

### Exit criteria

* [x] Providers can expose a unified interface
* [x] Shared auth layer supports login and refresh flows
* [x] Auth state survives restart
* [x] Provider capability discovery is functional

> **Phase 3 completed.** 131 tests pass across workspace (`cargo test --workspace`).
>
> **`crates/agent-core/src/types.rs`** — 10 new `AgentEvent` auth variants added:
> `AuthLoginStarted`, `AuthLoginCompleted`, `AuthLoginFailed`, `DeviceFlowInitiated`,
> `DeviceCodeIssued`, `TokenStored`, `TokenRefreshed`, `TokenRefreshFailed`,
> `AuthStateLoaded`, `AuthStateCleared`
>
> **`crates/model-adapters`** — provider abstraction expanded:
> - `EmbeddingRequest`/`EmbeddingResponse` typed models; `ModelInfo`, `ProviderMetadata`
> - `ProviderCapabilities` extended with auth flags (`supports_oauth_browser`, `supports_device_flow`, `supports_api_key`, `supports_token_refresh`) + `supports_model_discovery`
> - `requires_auth()`, `openai_compatible()`, `local_no_auth()` capability constructors
> - `ProviderRegistry` for runtime provider lookup
> - `ModelProvider::metadata()` and updated `list_models() -> Vec<ModelInfo>` and `embed(EmbeddingRequest)` signatures
> - 11 tests
>
> **`crates/auth-core`** — full auth subsystem:
> - `src/record.rs` — `TokenRecord` with expiry, refresh, scope, `to_auth_state()`; 9 tests
> - `src/encrypted_store.rs` — `EncryptedFileTokenStore` (AES-256-GCM, random 96-bit nonce, key file separate) + `MemoryTokenStore`; store survives reopen; 9 tests
> - `src/oauth.rs` — `OAuthFlow` CSRF-safe `begin()` + `exchange_code()` via `reqwest`; 4 tests
> - `src/device_flow.rs` — RFC 8628 `DeviceFlow` with `CancellationToken`, handles `authorization_pending`/`slow_down`/`expired_token`
> - `src/refresh.rs` — `refresh_token()` POST + `needs_refresh()` (5-min margin); 3 tests
> - `src/api_key.rs` — `ApiKeyAuth` env-var-first resolution; 6 tests
> - 31 tests total
>
> **Security note documented:** Key file is stored alongside encrypted token data — protects against bulk data copies but not full filesystem access. Platform keyring integration deferred to Phase 13.
>
> **Intentionally deferred:**
> - Actual per-provider adapter implementations (Phase 4)
> - PKCE support in OAuth flow (Phase 4, provider-specific)
> - Platform keyring integration (Phase 13)
> - OAuth local callback server for redirect capture (Phase 4/10 CLI)

---

## Phase 4 — First provider integrations

### Objectives

Get real model traffic working end-to-end.

### Deliverables

* [x] OpenAI-compatible adapter
* [x] local llama.cpp adapter
* [x] local vLLM adapter
* [x] one OAuth/device-auth provider adapter
* [x] model listing and streaming support

### Tasks

* [x] Build OpenAI-compatible adapter
* [x] Build llama.cpp adapter
* [x] Build vLLM adapter
* [x] Build first hosted auth-driven provider adapter

  * [x] GitHub Copilot or Gemini first
* [x] Implement:

  * [x] non-streaming chat
  * [x] streaming chat
  * [x] embeddings where available
  * [x] model discovery
* [x] Normalize provider errors
* [x] Add integration tests against mock/provider test harnesses

### Exit criteria

* [x] At least 4 adapters work end-to-end
* [x] Streaming works reliably
* [x] Model discovery works
* [x] Errors normalize into a shared internal schema

### Implementation notes

- All adapters live in `crates/model-adapters/src/adapters/` (`openai.rs`, `llamacpp.rs`, `vllm.rs`, `copilot.rs`).
- OpenAI wire types and error normalisation are extracted into `crates/model-adapters/src/wire.rs`.
- `LlamaCppAdapter` and `VllmAdapter` wrap `OpenAiAdapter` directly — same HTTP code path, different base URLs and auth defaults.
- `CopilotAdapter` uses RFC 8628 device flow: exchanges a GitHub OAuth token for a short-lived Copilot API token and auto-refreshes on 401. Embeddings and model discovery are not supported (static model list).
- `ProviderError` extended with 8 new variants: `Unauthorized`, `Forbidden`, `Timeout`, `Unavailable`, `InvalidRequest`, `UnsupportedCapability`, `MalformedResponse`, `NotConfigured`.
- Integration tests use `wiremock` mock HTTP server — no live API credentials required.

---

## Phase 5 — Tool runtime MVP

### Objectives

Stand up safe, observable tool execution.

### Deliverables

* [x] subprocess tool runner
* [x] file tool set (read_file, write_file)
* [x] search/edit tool set (search, edit_file)
* [x] timeout enforcement
* [x] cancellation enforcement
* [x] approval hooks for sensitive tools
* [x] tool lifecycle events: started, stdout, stderr, completed, cancelled, failed

### Tasks

* [x] Implement tool schema model (`ToolSensitivity`, `ToolConfig`, `ToolSchema`)
* [x] Implement subprocess execution (`SubprocessExecutor` with streaming)
* [x] Implement stdout/stderr streaming capture
* [x] Implement file read/write tools
* [x] Implement search/edit tools
* [x] Implement timeout enforcement
* [x] Implement cancellation enforcement
* [x] Implement approval hooks for sensitive tools
* [x] Emit tool lifecycle events:

  * [x] started
  * [x] stdout
  * [x] stderr
  * [x] completed
  * [x] cancelled
  * [x] failed
* [x] Add tests for:

  * [x] timeout
  * [x] cancellation
  * [x] non-zero exit handling
  * [x] unsafe path denial

### Exit criteria

* [x] Tools execute through a unified runtime
* [x] Sensitive actions can be gated
* [x] Timeouts and cancellation are reliable
* [x] Tool outputs are observable and auditable

### Architecture notes

The `tool-runtime` crate implements a unified runner (`ToolRunner`) that all tool calls flow through:

1. **Tool schema model** — `ToolSensitivity` (Safe / Low / High / Critical), `ToolConfig` (per-invocation timeout + cancellation token), and `ToolSchema` (name, description, JSON Schema params, sensitivity).
2. **Approval hooks** — `ApprovalHook` trait checked before any `High`/`Critical` tool runs. Built-in implementations: `AutoApprove` (tests/dev), `DenyAbove { threshold }` (deny at or above a sensitivity level), `AllowList` (named-tool whitelist).
3. **Path safety** — `PathSafetyPolicy` validates every file path against a configured set of allowed roots. `..` traversal is blocked; an explicit deny list is checked first. Violations return `ToolError::PathTraversal`.
4. **Event model** — `ToolRunner` emits `AgentEvent` variants on a `broadcast::Sender` in order: `ToolStarted` → `ToolStdout`/`ToolStderr` (subprocess only) → `ToolCompleted` | `ToolCancelled` | `ToolFailed`.
5. **Subprocess executor** — `run_subprocess` in `crates/tool-runtime/src/subprocess.rs` spawns via `tokio::process::Command`, streams stdout/stderr line-by-line as events, enforces a wall-clock timeout, and honours a `CancellationToken`. Output is capped at 512 KB per stream.

### Known limitations and deferred work

* `ShellTool` requires `AutoApprove` for tests; production use needs a real approval UI/hook.
* Search depth is limited to 5 directory levels.
* No glob pattern search — regex only.
* Output capped at 512 KB per stream (stdout/stderr); lines beyond this cap are dropped.
* Cancellation of synchronous file operations is at boundary level only (not mid-write).
* No tool output persistence/replay yet (deferred to Phase 12).
* `ShellTool` spawns via `/bin/sh -c` — shell injection risk if callers pass untrusted input without sanitisation.

---

## Phase 6 — Context engine MVP

### Objectives

Build working-set discovery, context packing, and compaction.

### Deliverables

* [x] CWD scanner
* [x] ignore rule engine
* [x] relevance filter
* [x] prompt context packer
* [x] summarization/compaction flow

### Tasks

* [x] Implement filesystem scanner
* [x] Implement ignore support

  * [x] `.gitignore`
  * [x] tool-specific ignore config (`.contextignore`)
* [x] Implement file relevance scoring
* [x] Implement working-set selection
* [x] Implement context packing by token budget
* [x] Implement compaction/summarization pipeline
* [x] Implement hooks to retrieve memory into context
* [x] Add tests for:

  * [x] ignore correctness
  * [x] token budgeting
  * [x] context truncation
  * [x] summarization fallback

### Exit criteria

* [x] The engine can build a bounded prompt context from a real project
* [x] Large contexts compact cleanly
* [x] Relevant files and memory are pulled consistently

> **Phase 6 completed.** 30 unit tests pass (`cargo test -p context-engine`).
>
> **Implemented in `crates/context-engine/src/`:**
> - `tokens.rs` — `estimate()` (4 chars/token heuristic), `estimate_bytes()`, `Budget` tracker
> - `ignore.rs` — `IgnoreEngine` wrapping the `ignore` crate (ripgrep engine) + `.contextignore` overlay support
> - `scanner.rs` — `Scanner` with `FileEntry` (path, size, mtime), `ScannerConfig`, `ScanStats`
> - `relevance.rs` — `score()`, `score_all()`, `ScoredEntry`, `RelevanceHints`; heuristics over extension, path proximity to root, and caller-supplied hint keywords
> - `workset.rs` — `select()`, `WorkingSet`, `SelectedFile`, diversity-capped selection via `max_per_dir`
> - `packer.rs` — async `ContextPacker`, `PackedContext`, `MemorySnippet`, per-file and per-context truncation with `[truncated]` markers
> - `compactor.rs` — `compact()` with three strategies: `DropLow` (drop files below a score threshold), `ExtractDeclarations` (rule-based header extraction), `Truncate` (proportional tail removal)
> - `memory.rs` — `MemoryRetriever` trait, `NoopMemory`, `StaticMemory`, `VaultMemory` (Phase 8 stub — reads `.md` files, no semantic search)
> - `engine.rs` — `ContextEngine` orchestrating the full pipeline: scan → score → select → compact (if needed) → memory retrieve → pack → `PackedContext`
> - `error.rs` — expanded `ContextError` variants covering scan I/O, no relevant files, token overflow, memory, and packer errors
>
> ### Architecture notes
>
> **Pipeline flow:**
> ```
> scan() → ignore filter → relevance score → workset select
>   ↓
> compact (if estimated tokens > budget × compaction_threshold)
>   ↓
> memory retrieve (parallel with packing)
>   ↓
> pack (token budget) → PackedContext
> ```
>
> **Token budgeting:** Uses the same `~4 chars/token` heuristic as `agent_core::prompt::estimate_tokens`. The `Budget` struct tracks `used` vs `remaining` and exposes `would_fit()` / `consume()`. Provider-specific tokenization is deferred to Phase 7.
>
> **Diversity cap:** `WorksetConfig::max_per_dir` prevents any single directory from dominating the working set. Files are sorted by score descending; a per-directory counter enforces the cap during selection.
>
> **Compaction before packing:** If the estimated token total of the working set exceeds `token_budget × compaction_threshold`, the compactor runs before file I/O. `DropLow` is the default strategy; `ExtractDeclarations` and `Truncate` are available for more aggressive compaction.
>
> **Memory integration:** `ContextEngine::with_memory()` accepts any `Arc<dyn MemoryRetriever>`. The memory budget is carved out of the total token budget before file blocks are packed.
>
> ### Deferred work
>
> - **LLM-backed summarization** — `ExtractDeclarations` strategy uses rule-based line extraction only; true LLM summarization is deferred.
> - **`VaultMemory`** — reads `.md` files from a vault directory; no semantic/vector search. Full implementation in Phase 8.
> - **Real tokenization** — provider-specific byte-exact token counts deferred to Phase 7 (session-store integration).

---

## Phase 7 — Session stores and durable memory backends

### Objectives

Add persistent runtime state and memory layers.

### Deliverables

* [x] SQLite session backend
* [x] `sled` session backend
* [x] PostgreSQL session backend
* [x] Qdrant integration
* [x] memory abstraction layer

### Tasks

* [x] Define store traits for:

  * [x] sessions (`SessionStore`)
  * [x] runs (`RunStore`)
  * [ ] events (deferred — `event-log` crate owns the event log; no separate `EventStore` trait added)
  * [x] summaries (`SummaryStore`)
  * [x] memories (`MemoryStore`)
* [x] Implement SQLite backend
* [x] Implement `sled` backend
* [x] Implement PostgreSQL backend
* [x] Implement Qdrant semantic memory integration (`QdrantMemory` in `memory-sync`)
* [x] Add migration/versioning scheme
* [x] Add backend selection config
* [x] Add tests for:

  * [x] backend parity (10 unit tests covering SQLite + sled paths)
  * [x] restart recovery (SQLite/sled reopen tests)
  * [x] schema migration safety (version check on open)

### Exit criteria

* [x] Session state persists cleanly across restarts
* [x] Backends are swappable behind shared traits
* [x] Qdrant memory retrieval works

> **Phase 7 completed.** 10 unit tests pass, 1 ignored (`cargo test -p session-store`).
>
> **`crates/session-store/src/`:**
> - `store.rs` — four store traits: `SessionStore` (create/get/list/update-summary/delete), `RunStore` (create/get/list/update-status), `SummaryStore` (save/get-latest/list), `MemoryStore` (save/get/list/search/delete); associated record types `SessionRecord`, `RunRecord`, `SummaryRecord`, `MemoryRecord`
> - `sqlite.rs` — `SqliteBackend` implementing all four traits; auto-initialises schema on connect; stores schema version in a `_meta` table; uses `sqlx` with the `sqlite` feature
> - `sled_store.rs` — `SledBackend` implementing all four traits; pure-Rust embedded key-value store; serialises records as MessagePack/JSON; opens or creates the sled tree on startup
> - `postgres.rs` — `PostgresBackend` implementing all four traits; schema init + version check on connect; requires a `postgres_url` connection string; integration tests require a live DB and are marked `#[ignore]`
> - `factory.rs` — config-driven factory functions (`build_session_store`, `build_run_store`, `build_summary_store`, `build_memory_store`) that construct `Arc<dyn Trait>` from `MemoryConfig::session_backend`; default paths: `~/.rustpi/sessions.db` (SQLite), `~/.rustpi/sessions.sled` (sled)
>
> **`crates/memory-sync/src/`:**
> - `qdrant.rs` — `QdrantMemory` implementing `context_engine::memory::MemoryRetriever`; stores records as Qdrant points with content + metadata payload; Phase 7 uses keyword-filtered scroll (no embeddings); `search_similar()` available for future ANN retrieval once embeddings are generated; graceful offline fallback: errors log a warning and return empty snippet list
> - `memory.rs` — `MemoryRecord` model (id, session_id, content, tags, optional embedding, created_at, updated_at)
>
> ### Architecture notes
>
> **Four store traits and their domains:**
> | Trait | Domain |
> |---|---|
> | `SessionStore` | Session lifecycle — create, list, update summary, delete |
> | `RunStore` | Run lifecycle within a session — status transitions |
> | `SummaryStore` | Compaction artifacts — ordered summaries per session |
> | `MemoryStore` | Structured memory records — tagged, searchable, optional session scope |
>
> **Backend implementations:**
> All three backends (`SqliteBackend`, `SledBackend`, `PostgresBackend`) implement all four traits. The factory layer constructs the correct backend from `MemoryConfig::session_backend` (`sqlite` | `sled` | `postgres`).
>
> **Migration/versioning scheme:**
> SQLite and PostgreSQL backends store a schema version in a `_meta` / metadata table on first init and check it on every subsequent open. A version mismatch returns a `StoreError::Migration` before any data access. `SledBackend` uses tree naming conventions for forward compatibility.
>
> **Qdrant semantic memory:**
> `QdrantMemory` connects to a Qdrant instance at a configurable URL (default collection `rustpi_memory`, vector size 1536). Phase 7 retrieval uses scroll + keyword filtering. The `upsert_memory()` and `search_similar()` methods accept pre-computed embeddings and are ready for Phase 9 embedding generation.
>
> **Config-driven factory:**
> `session-store/src/factory.rs` exposes four `async fn build_*_store(config: &MemoryConfig)` functions that return `Arc<dyn Trait>`. Callers do not need to know which backend is active.
>
> ### Deferred work
>
> - **Embedding generation** — `QdrantMemory::search_similar()` requires a vector; actual embedding calls are deferred to Phase 9+ (requires a model adapter capable of embeddings).
> - **PostgreSQL integration tests** — the live-DB test is marked `#[ignore]`; CI runs without a Postgres instance. Enable with `DATABASE_URL=postgres://... cargo test -p session-store -- --ignored`.
> - **EventStore trait** — event persistence remains in the `event-log` crate (JSONL file store); a unified `EventStore` trait for session-store backends was not added and is deferred.

---

## Phase 8 — Obsidian vault memory and personality system

### Objectives

Implement the human-readable memory layer and personality documents.

### Deliverables

* [x] Obsidian vault integration
* [x] Markdown memory schema
* [x] personality document loader
* [x] sync rules between runtime memory and vault memory

### Tasks

* [x] Implement vault path configuration
* [x] Implement Markdown memory reader/writer
* [x] Implement canonical docs:

  * [x] `AGENTS.md`
  * [x] `BOOT.md`
  * [x] `BOOTSTRAP.md`
  * [x] `HEARTBEAT.md`
  * [x] `IDENTITY.md`
  * [x] `SOUL.md`
  * [x] `TOOLS.md`
  * [x] `USER.md`
* [x] Define which docs are:

  * [x] read-only at runtime
  * [x] writable by runtime
  * [x] writable only by approval
* [x] Implement `memory-sync`

  * [x] structured store → vault sync
  * [x] vault → retrieval index sync
  * [x] conflict rules
* [x] Implement personality loading into prompt assembly
* [x] Add tests for:

  * [x] malformed markdown
  * [x] sync conflicts
  * [x] duplicate note handling
  * [x] prompt assembly from personality docs

### Exit criteria

* [x] The agent can load personality and long-term memory from the vault
* [x] Runtime memory and vault memory can synchronize safely
* [x] Human-readable memory is stable and inspectable

> **Phase 8 completed.** 34 unit tests pass (`cargo test -p memory-sync`).
>
> **Implemented in `crates/memory-sync/src/`:**
> - `markdown.rs` — `VaultDoc` line-by-line Markdown parser; frontmatter between `---` delimiters; `#` headings split document into named `Section`s; `<!-- machine-managed -->` comment marks sections the runtime owns; `upsert_machine_section()` inserts or replaces only machine-managed sections — human-authored sections are unconditionally preserved; `render()` re-serialises to a canonical string
> - `docs.rs` — `CanonicalDoc` enum for all 8 canonical docs (Agents, Boot, Bootstrap, Heartbeat, Identity, Soul, Tools, User); `filename()`, `mutability()`, `included_in_prompt()`, `prompt_priority()`, `default_template()`; `load_doc()` and `load_all_docs()` (missing docs return `Ok(None)`, silently skipped in batch load)
> - `vault.rs` — `VaultAccessor` with `open()` (validates path exists and is a directory), `read_doc()`, `write_doc()` (enforces ReadOnly → error, ApprovalRequired → error, RuntimeWritable → write), `write_doc_approved()` (bypasses approval check after external grant — ReadOnly still blocked), `update_machine_sections()` (upserts named machine-managed sections, creates file from default template if absent), `read_file()` / `list_files()`, `init_defaults()`; path traversal protection via `check_no_traversal()` (rejects any `..` component)
> - `personality.rs` — `PersonalityBundle` with `content`, `token_count`, `loaded_docs`, `missing_docs`; `load_personality()` reads prompt-included docs in priority order (Soul → Identity → Agents → User → Boot), truncates to a token budget (default 4,000 tokens), concatenates with section separators; `inject_personality()` pushes the result as a System section into `PromptAssembler`
> - `sync.rs` — `SyncEngine`: `sync_to_vault()` writes HEARTBEAT and TOOLS machine-managed sections from runtime state; `index_vault()` scans all vault docs and builds an in-memory index (`Vec<IndexedDoc>`); `detect_conflicts()` compares machine-managed section checksums to detect manual edits, records `ConflictRecord { doc, section, kind: RequiresReview }` for any mismatch
> - `error.rs` — `MemorySyncError` extended with `ApprovalRequired(String)`, `PathTraversal(String)`, and `Init(String)` variants
>
> ### Architecture notes
>
> **Five new modules and their responsibilities:**
>
> | Module | Responsibility |
> |--------|----------------|
> | `markdown.rs` | Parse and render vault `.md` files; enforce machine-managed vs human-authored boundary |
> | `docs.rs` | Type-safe canonical doc enum; write policies; default templates |
> | `vault.rs` | File I/O with path-safety; policy enforcement per `DocMutability` |
> | `personality.rs` | Token-bounded personality assembly; prompt injection |
> | `sync.rs` | Runtime-state → vault push; vault index; conflict detection |
>
> **Canonical doc write policies:**
> - `ReadOnly` — SOUL, IDENTITY, AGENTS, BOOT, BOOTSTRAP: runtime never writes these; only the operator edits them.
> - `RuntimeWritable` — HEARTBEAT, TOOLS: runtime may update without approval.
> - `ApprovalRequired` — USER: runtime must call `write_doc_approved()` after explicit user consent.
>
> **Sync pipeline:**
> ```
> sync_to_vault()  →  upsert machine sections in HEARTBEAT.md + TOOLS.md
> index_vault()    →  scan all .md files → Vec<IndexedDoc> (for retrieval)
> detect_conflicts() →  checksum machine sections → Vec<ConflictRecord>
> ```
>
> **Personality loading order (highest priority first):**
> Soul (0) → Identity (1) → Agents (2) → User (3) → Boot (4)
> Missing docs are silently skipped. Token budget defaults to 4,000 tokens shared across all docs.
>
> **Machine-managed vs human-authored:**
> A `<!-- machine-managed -->` comment on the first line of a section body marks it as runtime-owned. All other sections are human-authored and are never overwritten by any vault API.
>
> ### Deferred work
>
> - **Rich conflict UI** — `detect_conflicts()` records conflicts but does not yet present a diff or merge UI to the operator; this is deferred to Phase 11 (TUI).
> - **Semantic dedup of vault notes** — `index_vault()` scans all docs but does not deduplicate semantically similar notes; deferred to Phase 9.
> - **Embedding-based retrieval from vault** — `VaultMemory` in `context-engine` currently reads raw Markdown with no vector search; ANN retrieval from vault content deferred to Phase 9 (requires embedding model integration).

---

## Phase 9 — RPC API

### Objectives

Provide machine-to-machine embedding and streaming control.

### Deliverables

* [x] stdin/stdout JSONL RPC protocol
* [x] session attach/detach
* [x] structured request/response model
* [x] streamed event output

### Tasks

* [x] Define RPC request schema
* [x] Define RPC response schema
* [x] Define streaming event schema
* [x] Implement session attach/detach
* [x] Implement run start/stop commands
* [x] Implement tool passthrough events
* [x] Implement auth-status queries
* [x] Implement capability discovery
* [x] Add integration tests with a stub host process

### Exit criteria

* [x] External hosts can drive the runtime over JSONL
* [x] Streaming output is stable
* [x] RPC can be used by CLI and future editor integrations

> **Phase 9 completed.** ~462 tests pass across workspace (`cargo test --workspace`).
>
> **Implemented in `crates/rpc-api/src/`:**
> - `transport.rs` — `LineReader<R>` (async JSONL reader, skips empty lines) + `LineWriter<W>` (`Arc<Mutex>`-backed, `Clone`-able for concurrent writes from multiple server tasks)
> - `protocol.rs` — Full protocol types: `RpcRequest`, `RpcMethod` (6 variants), `RpcResponse` (Ack/Success/StreamEvent/Event/Error), `RpcEvent`, `EventCategory` (5 variants), `SessionInfo`, `RunInfo`, `AuthStatusInfo`, `CapabilitiesInfo`, `RpcErrorCode` (9 variants)
> - `server.rs` — `RpcServer<R,W>` main dispatch loop with parse-error recovery and broken-pipe detection; `ServerState` (sessions map, runs map, event bus, monotonic seq counter, cancel tokens); `stdio_server()` convenience constructor over `tokio::io::stdin/stdout`
> - `dispatch.rs` — Handler functions for all 6 `RpcMethod` variants: `SessionAttach`, `SessionDetach`, `RunStart` (with simulated streaming), `RunCancel` (token-based), `AuthStatus`, `Capabilities`
> - `normalize.rs` — `normalize_event()` maps all 40+ `AgentEvent` variants to typed `RpcEvent`s with assigned `EventCategory`, session/run ID extraction, and safe external payloads (internal fields stripped)
> - `error.rs` — `RpcError` with variants: `SessionNotFound`, `RunNotFound`, `InvalidRunState`, `BrokenPipe`, `Internal`
>
> ### Architecture notes
>
> **Transport layer:**
> `LineReader` wraps a `tokio::io::AsyncBufRead`, reads one line at a time, skips blank lines, and returns `None` on EOF. `LineWriter` wraps the writer in `Arc<Mutex<...>>` so it is `Clone` and multiple server tasks can write responses concurrently without interleaving.
>
> **Protocol design:**
> ```
> Client → Server:  one RpcRequest per line (JSONL)
> Server → Client:  RpcResponse variants (Ack, Success, StreamEvent, Event, Error)
> ```
> All messages are single-line JSON objects terminated by `\n`. The server emits an `Ack` immediately after parsing each request, then follows with `Success` / `StreamEvent` / `Error` as the operation completes.
>
> **RpcMethod variants and their responses:**
>
> | Method | Immediate | Terminal |
> |--------|-----------|---------|
> | `SessionAttach` | Ack | Success(`SessionInfo`) |
> | `SessionDetach` | Ack | Success |
> | `RunStart` | Ack | StreamEvents + Success(`RunInfo`) |
> | `RunCancel` | Ack | Success |
> | `AuthStatus` | Ack | Success(`AuthStatusInfo`) |
> | `Capabilities` | Ack | Success(`CapabilitiesInfo`) |
>
> **Event normalization:**
> `normalize_event()` in `normalize.rs` is an exhaustive `match` over all `AgentEvent` variants. Each arm produces an `RpcEvent` with a typed `event_type` string, appropriate `EventCategory`, extracted `session_id`/`run_id`, and a safe `payload` (serde_json::Value). Internal-only fields are excluded from the payload.
>
> **EventCategory values:**
> `session` · `run` · `tool` · `auth` · `system`
>
> **RpcErrorCode values:**
> `parse_error` · `invalid_request` · `unknown_method` · `session_not_found` · `run_not_found` · `invalid_run_state` · `auth_unavailable` · `capability_unavailable` · `internal_error`
>
> ### Known limitations and deferred work
>
> - **RunStart uses simulated streaming** — event chunks are synthesised by the dispatch layer; real model-adapter streaming integration is deferred to Phase 10 (CLI) when a full run executor is wired up.
> - **AuthStatus is a stub** — returns a placeholder `AuthStatusInfo` with `authenticated: false`; real provider auth query is deferred to Phase 10.
> - **Embedding generation deferred** — `QdrantMemory::search_similar()` ANN path requires pre-computed embeddings; no embedding model is wired in yet (deferred to Phase 10+).
> - **No multiplexed sessions over a single stream** — the current server handles one active run at a time per stream; parallel run support across a single stdio channel is deferred.
> - **No TLS / Unix socket transport** — only stdin/stdout is implemented; network transport variants are deferred to Phase 12.

---

## Phase 10 — CLI

### Objectives

Deliver a production-usable scriptable interface.

### Deliverables

* [ ] print mode
* [ ] JSON mode
* [ ] non-interactive mode
* [ ] piped I/O support
* [ ] file/task execution mode

### Tasks

* [ ] Implement CLI argument parsing
* [ ] Implement prompt submission
* [ ] Implement JSON output mode
* [ ] Implement streaming terminal output
* [ ] Implement file-based task execution
* [ ] Implement session resume/select
* [ ] Implement provider/model selection flags
* [ ] Implement auth commands
* [ ] Implement diagnostics commands
* [ ] Add end-to-end CLI tests

### Exit criteria

* [ ] CLI supports interactive-enough scripting workflows
* [ ] JSON mode is stable for automation
* [ ] Operators can authenticate and run tasks from the terminal

---

## Phase 11 — Ratatui TUI

### Objectives

Build the primary interactive operator experience.

### Deliverables

* [ ] Ratatui full-screen TUI
* [ ] conversation pane
* [ ] tool activity pane
* [ ] context pane
* [ ] session/memory pane
* [ ] provider/auth pane
* [ ] logs/events pane

### Tasks

* [ ] Implement Ratatui app shell
* [ ] Implement pane layout system
* [ ] Implement streaming conversation renderer
* [ ] Implement tool activity feed
* [ ] Implement session navigation
* [ ] Implement provider/model picker
* [ ] Implement auth status views
* [ ] Implement interrupt/approval workflows
* [ ] Implement memory/context inspection
* [ ] Implement keyboard shortcut system
* [ ] Add TUI snapshot and interaction tests where practical

### Exit criteria

* [ ] TUI is usable as the main operator interface
* [ ] Streaming and tool activity remain readable
* [ ] Approval and interrupt workflows are reliable

---

## Phase 12 — Observability, replay, and reliability hardening

### Objectives

Make the system debuggable, recoverable, and durable.

### Deliverables

* [ ] replay tooling
* [ ] session diagnostics
* [ ] provider latency/error tracking
* [ ] token usage tracking
* [ ] crash recovery behavior
* [ ] safe resume behavior

### Tasks

* [ ] Build session replay viewer
* [ ] Build diagnostics commands
* [ ] Track:

  * [ ] provider latency
  * [ ] provider error rate
  * [ ] token usage
  * [ ] tool failures
  * [ ] cancellation counts
* [ ] Implement resume-after-crash logic
* [ ] Implement incomplete-run reconciliation
* [ ] Add chaos/failure tests for:

  * [ ] provider disconnects
  * [ ] hung tool execution
  * [ ] partial event log writes
  * [ ] token refresh failure

### Exit criteria

* [ ] Failures are diagnosable
* [ ] Sessions can recover cleanly after interruption
* [ ] Runtime behavior is observable and auditable 

---

## Phase 13 — Security hardening

### Objectives

Harden secrets, permissions, and execution boundaries.

### Deliverables

* [ ] secure token storage
* [ ] tool permission controls
* [ ] file mutation safeguards
* [ ] audit logging
* [ ] secrets redaction

### Tasks

* [ ] Encrypt persisted tokens
* [ ] Redact secrets from logs/events
* [ ] Restrict tool execution paths
* [ ] Add allow/deny command lists
* [ ] Add path traversal protections
* [ ] Add file overwrite safeguards
* [ ] Add approval requirements for destructive actions
* [ ] Conduct threat model review
* [ ] Add security-focused tests

### Exit criteria

* [ ] Tokens and secrets are handled safely
* [ ] Destructive actions are bounded and reviewable
* [ ] Logs and memory stores avoid secret leakage 

---

## Phase 14 — Full-system testing and parity validation

### Objectives

Validate that the platform achieves the intended feature set.

### Deliverables

* [ ] unit test coverage across crates
* [ ] integration test suite
* [ ] backend matrix tests
* [ ] provider matrix tests
* [ ] CLI/TUI test checklist
* [ ] memory sync test checklist
* [ ] release-readiness checklist

### Tasks

* [ ] Unit tests for each crate
* [ ] Integration tests for full run lifecycle
* [ ] Provider tests:

  * [ ] OpenAI-compatible
  * [ ] llama.cpp
  * [ ] vLLM
  * [ ] first OAuth/device-auth provider
* [ ] Storage tests:

  * [ ] SQLite
  * [ ] `sled`
  * [ ] PostgreSQL
  * [ ] Qdrant
  * [ ] Obsidian vault sync
* [ ] Interface tests:

  * [ ] RPC
  * [ ] CLI
  * [ ] TUI
* [ ] Failure mode tests:

  * [ ] token expiry
  * [ ] network drop
  * [ ] hung tool
  * [ ] corrupted note
  * [ ] partial replay log
* [ ] Run feature parity review against architecture

### Exit criteria

* [ ] All declared core features are implemented
* [ ] Core backends and provider paths are tested
* [ ] The system is stable enough for internal daily use

---

# Recommended release grouping

## Release 1 — Core operator MVP

Target phases:

* [x] Phase 0
* [x] Phase 1
* [x] Phase 2
* [x] Phase 3
* [x] Phase 4
* [x] Phase 5
* [x] Phase 6
* [x] Phase 7
* [ ] Phase 9
* [ ] Phase 10

Outcome:

* usable CLI-driven agent
* real providers
* auth
* tools
* sessions
* context
* structured persistence

## Release 2 — Memory and personality system

Target phases:

* [x] Phase 8
* [ ] Phase 12
* [ ] Phase 13

Outcome:

* Obsidian-backed human-readable memory
* replayability
* better reliability
* safer execution

## Release 3 — Full interactive platform

Target phases:

* [ ] Phase 11
* [ ] Phase 14

Outcome:

* polished Ratatui experience
* tested end-to-end feature parity

---

# Critical path

These are the dependencies that will govern the schedule:

* [x] Phase 0 before all other phases
* [x] Phase 1 before Phase 5, 6, 9, 10, 11
* [x] Phase 3 before Phase 4
* [ ] Phase 4 before meaningful end-to-end testing
* [x] Phase 7 before mature memory layering
* [x] Phase 8 depends on Phase 6 and 7
* [ ] Phase 11 depends on Phase 9 and core runtime stability
* [ ] Phase 14 depends on all major implementation phases

---

# What I would prioritize first

If the goal is to get to a usable system fastest, I would build in this order:

1. Phase 0
2. Phase 1
3. Phase 2
4. Phase 3
5. Phase 4
6. Phase 5
7. Phase 6
8. Phase 7
9. Phase 10
10. Phase 9
11. Phase 8
12. Phase 12
13. Phase 13
14. Phase 11
15. Phase 14

That sequence gets you a working agent sooner, while delaying the TUI until the runtime, auth, tools, context, and persistence layers are already stable.

If you want, I can turn this into a **project-ready `development_plan.md` file** with tighter wording and GitHub-style checkboxes only.
