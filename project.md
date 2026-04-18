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

* [ ] OpenAI-compatible adapter
* [ ] local llama.cpp adapter
* [ ] local vLLM adapter
* [ ] one OAuth/device-auth provider adapter
* [ ] model listing and streaming support

### Tasks

* [ ] Build OpenAI-compatible adapter
* [ ] Build llama.cpp adapter
* [ ] Build vLLM adapter
* [ ] Build first hosted auth-driven provider adapter

  * [ ] GitHub Copilot or Gemini first
* [ ] Implement:

  * [ ] non-streaming chat
  * [ ] streaming chat
  * [ ] embeddings where available
  * [ ] model discovery
* [ ] Normalize provider errors
* [ ] Add integration tests against mock/provider test harnesses

### Exit criteria

* [ ] At least 4 adapters work end-to-end
* [ ] Streaming works reliably
* [ ] Model discovery works
* [ ] Errors normalize into a shared internal schema

---

## Phase 5 — Tool runtime MVP

### Objectives

Stand up safe, observable tool execution.

### Deliverables

* [ ] subprocess tool runner
* [ ] file tool set
* [ ] search/edit tool set
* [ ] timeout/cancellation enforcement
* [ ] tool event emission

### Tasks

* [ ] Implement tool schema model
* [ ] Implement subprocess execution
* [ ] Implement stdout/stderr streaming capture
* [ ] Implement file read/write tools
* [ ] Implement search/edit tools
* [ ] Implement timeout enforcement
* [ ] Implement cancellation enforcement
* [ ] Implement approval hooks for sensitive tools
* [ ] Emit tool lifecycle events:

  * [ ] started
  * [ ] stdout
  * [ ] stderr
  * [ ] completed
  * [ ] cancelled
  * [ ] failed
* [ ] Add tests for:

  * [ ] timeout
  * [ ] cancellation
  * [ ] non-zero exit handling
  * [ ] unsafe path denial

### Exit criteria

* [ ] Tools execute through a unified runtime
* [ ] Sensitive actions can be gated
* [ ] Timeouts and cancellation are reliable
* [ ] Tool outputs are observable and auditable

---

## Phase 6 — Context engine MVP

### Objectives

Build working-set discovery, context packing, and compaction.

### Deliverables

* [ ] CWD scanner
* [ ] ignore rule engine
* [ ] relevance filter
* [ ] prompt context packer
* [ ] summarization/compaction flow

### Tasks

* [ ] Implement filesystem scanner
* [ ] Implement ignore support

  * [ ] `.gitignore`
  * [ ] tool-specific ignore config
* [ ] Implement file relevance scoring
* [ ] Implement working-set selection
* [ ] Implement context packing by token budget
* [ ] Implement compaction/summarization pipeline
* [ ] Implement hooks to retrieve memory into context
* [ ] Add tests for:

  * [ ] ignore correctness
  * [ ] token budgeting
  * [ ] context truncation
  * [ ] summarization fallback

### Exit criteria

* [ ] The engine can build a bounded prompt context from a real project
* [ ] Large contexts compact cleanly
* [ ] Relevant files and memory are pulled consistently

---

## Phase 7 — Session stores and durable memory backends

### Objectives

Add persistent runtime state and memory layers.

### Deliverables

* [ ] SQLite session backend
* [ ] `sled` session backend
* [ ] PostgreSQL session backend
* [ ] Qdrant integration
* [ ] memory abstraction layer

### Tasks

* [ ] Define store traits for:

  * [ ] sessions
  * [ ] runs
  * [ ] events
  * [ ] summaries
  * [ ] memories
* [ ] Implement SQLite backend
* [ ] Implement `sled` backend
* [ ] Implement PostgreSQL backend
* [ ] Implement Qdrant semantic memory integration
* [ ] Add migration/versioning scheme
* [ ] Add backend selection config
* [ ] Add tests for:

  * [ ] backend parity
  * [ ] restart recovery
  * [ ] schema migration safety

### Exit criteria

* [ ] Session state persists cleanly across restarts
* [ ] Backends are swappable behind shared traits
* [ ] Qdrant memory retrieval works

---

## Phase 8 — Obsidian vault memory and personality system

### Objectives

Implement the human-readable memory layer and personality documents.

### Deliverables

* [ ] Obsidian vault integration
* [ ] Markdown memory schema
* [ ] personality document loader
* [ ] sync rules between runtime memory and vault memory

### Tasks

* [ ] Implement vault path configuration
* [ ] Implement Markdown memory reader/writer
* [ ] Implement canonical docs:

  * [ ] `AGENTS.md`
  * [ ] `BOOT.md`
  * [ ] `BOOTSTRAP.md`
  * [ ] `HEARTBEAT.md`
  * [ ] `IDENTITY.md`
  * [ ] `SOUL.md`
  * [ ] `TOOLS.md`
  * [ ] `USER.md`
* [ ] Define which docs are:

  * [ ] read-only at runtime
  * [ ] writable by runtime
  * [ ] writable only by approval
* [ ] Implement `memory-sync`

  * [ ] structured store → vault sync
  * [ ] vault → retrieval index sync
  * [ ] conflict rules
* [ ] Implement personality loading into prompt assembly
* [ ] Add tests for:

  * [ ] malformed markdown
  * [ ] sync conflicts
  * [ ] duplicate note handling
  * [ ] prompt assembly from personality docs

### Exit criteria

* [ ] The agent can load personality and long-term memory from the vault
* [ ] Runtime memory and vault memory can synchronize safely
* [ ] Human-readable memory is stable and inspectable

---

## Phase 9 — RPC API

### Objectives

Provide machine-to-machine embedding and streaming control.

### Deliverables

* [ ] stdin/stdout JSONL RPC protocol
* [ ] session attach/detach
* [ ] structured request/response model
* [ ] streamed event output

### Tasks

* [ ] Define RPC request schema
* [ ] Define RPC response schema
* [ ] Define streaming event schema
* [ ] Implement session attach/detach
* [ ] Implement run start/stop commands
* [ ] Implement tool passthrough events
* [ ] Implement auth-status queries
* [ ] Implement capability discovery
* [ ] Add integration tests with a stub host process

### Exit criteria

* [ ] External hosts can drive the runtime over JSONL
* [ ] Streaming output is stable
* [ ] RPC can be used by CLI and future editor integrations

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
* [ ] Phase 4
* [ ] Phase 5
* [ ] Phase 6
* [ ] Phase 7
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

* [ ] Phase 8
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
* [ ] Phase 3 before Phase 4
* [ ] Phase 4 before meaningful end-to-end testing
* [ ] Phase 7 before mature memory layering
* [ ] Phase 8 depends on Phase 6 and 7
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
