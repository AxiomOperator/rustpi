# Release Readiness Checklist

Internal go/no-go checklist for the `rustpi` platform. Items marked **Automated** have test coverage in the workspace. Items marked **Manual** require a live environment, external service, or operator inspection.

---

## Core Runtime

- [ ] **Session creation/selection works**: Sessions can be created, listed, and attached. **Automated** → `crates/agent-core` session tests; `crates/cli/tests/cli_tests.rs` → `test_executor_session_attach_detach`, `test_executor_session_list`
- [ ] **Run lifecycle: start → running → completed/failed/cancelled**: All `RunStatus` FSM transitions are valid and invalid transitions are rejected. **Automated** → `crates/agent-core/src/run.rs` unit tests
- [ ] **Event bus delivers events to all subscribers**: `EventBus` fan-out reaches all broadcast subscribers; append-only log also receives all events. **Automated** → `crates/agent-core/src/bus.rs` unit tests
- [ ] **Event log persists and replays correctly**: JSONL event log can be written and replayed in order. **Automated** → `crates/event-log` tests; `rustpi replay` CLI tests → `test_replay_command_exits_zero`
- [ ] **Context engine builds bounded context within token budget**: `ContextEngine` assembles context without exceeding configured token limit. **Automated** → `crates/context-engine` tests
- [ ] **Compaction/summarization handles oversized context**: When context exceeds budget, compaction reduces it without dropping critical content silently. **Automated** → `crates/context-engine` compaction tests

---

## Provider Readiness

- [ ] **OpenAI-compatible: chat, streaming, models list, error normalization**: Send a chat request and streaming request to an OpenAI-compatible endpoint; verify response and error shapes. **Manual** (requires live API key)
- [ ] **llama.cpp: local server chat, fallback on server-down**: Chat works when `llama-server` is running; graceful error when server is not running. **Manual** (requires local `llama-server`)
- [ ] **vLLM: OpenAI-compatible chat, streaming**: Chat and streaming work against a vLLM endpoint. **Manual** (requires live vLLM)
- [ ] **Copilot: device-auth, token refresh, static model list**: Device-auth flow completes; token is refreshed before expiry; model list returns known Copilot models. **Manual** (requires GitHub Copilot access)
- [ ] **Auth token storage (AES-256-GCM encrypted)**: Tokens are persisted encrypted in `~/.config/rustpi/tokens.enc`; file is not plaintext. **Automated** → `crates/auth-core` `EncryptedFileTokenStore` tests; **Manual** (inspect file with `xxd`)
- [ ] **Token expiry detection and refresh**: When a token is within the refresh window, the provider triggers refresh before the next request. **Manual** (set token expiry to near-future, verify auto-refresh)

---

## Storage / Backend Readiness

- [ ] **SQLite backend: session/run/event/summary/memory CRUD**: All four store traits (`SessionStore`, `RunStore`, `EventStore`, `SummaryStore`) pass CRUD operations on SQLite. **Automated** → `crates/session-store` SQLite integration tests
- [ ] **sled backend: session/run/event/summary CRUD, restart recovery**: CRUD operations pass on sled; after simulated restart, in-flight runs are reconciled. **Automated** → `crates/session-store` sled tests
- [ ] **PostgreSQL backend: parity with SQLite**: All CRUD operations that pass on SQLite also pass on PostgreSQL. **Manual** (requires live PostgreSQL)
- [ ] **Qdrant semantic memory: upsert + query**: `QdrantMemory::upsert` and `QdrantMemory::query` return correct results. **Manual** (requires live Qdrant)
- [ ] **Vault sync: structured store ↔ vault ↔ retrieval index**: Full sync pipeline moves data correctly in both directions. **Automated** → `crates/memory-sync/tests/integration.rs` sync tests; **Manual** for Qdrant leg
- [ ] **Schema migration: safe startup on fresh DB**: Starting with no existing database creates the schema correctly without errors. **Automated** → `crates/session-store` startup tests

---

## RPC / CLI / TUI Readiness

- [ ] **RPC server: protocol framing, request dispatch, streaming events**: JSONL RPC server handles session attach/detach, run start/cancel, and streams `AgentEvent`s correctly. **Automated** → `crates/rpc-api` tests
- [ ] **CLI: all major commands work end-to-end**: `run`, `session list`, `session resume`, `auth status`, `diag`, `replay` all exit 0 on happy path. **Automated** → `crates/cli/tests/cli_tests.rs`
- [ ] **TUI: renders, navigates, handles approval/interrupt flows**: TUI state machine and pane renders work for all major event types and key bindings. **Automated** → `crates/tui/tests/tui_tests.rs`
- [ ] **JSON output mode**: `--output json` produces valid `{"ok": true/false, "data"/"error": ...}` envelope for all commands. **Automated** → `test_session_list_json`, `test_diag_json_valid`, `test_replay_json_output`
- [ ] **Piped I/O**: `echo "task" | rustpi run` uses piped stdin as prompt. **Manual**

---

## Observability / Replay

- [ ] **OpenTelemetry spans emitted for key operations**: Provider calls, tool executions, and session operations emit OTLP spans. **Automated** → `crates/observability` `TelemetryCollector` tests
- [ ] **Prometheus metrics exported**: `ProviderMetrics`, `ToolMetrics`, and `TokenUsageTracker` counters increment correctly. **Automated** → `crates/observability` metrics tests
- [ ] **Event log replay works for completed runs**: `rustpi replay` replays events from completed runs without error. **Automated** → `test_replay_command_exits_zero`, `test_replay_audit_flag`, `test_replay_failures_flag`
- [ ] **Crash recovery reconciles in-flight runs on restart**: After a simulated crash (kill -9), restarting `rustpi` moves orphaned `Running` runs to `Failed`. **Automated** → `crates/session-store` crash recovery tests; **Manual** (end-to-end)
- [ ] **Diagnostics command surfaces useful info**: `rustpi diag` output contains provider configuration, auth status, and version info. **Automated** → `test_diag_json_valid`

---

## Security Hardening

- [ ] **Secrets redacted from logs and events**: `Redactor` strips API keys and bearer tokens from all `AgentEvent` fields before logging. **Automated** → `crates/agent-core` `Redactor` tests (Phase 13)
- [ ] **Command policy denies dangerous shell commands**: `CommandPolicy` blocks commands on the deny list (e.g., `rm -rf /`). **Automated** → `crates/policy-engine` and `crates/tool-runtime` command policy tests
- [ ] **File overwrite safeguards active**: `OverwritePolicy` prevents writing to files not explicitly permitted. **Automated** → `crates/tool-runtime` overwrite policy tests
- [ ] **Audit events emitted for security-relevant actions**: Tool executions, auth operations, and policy decisions emit audit `AgentEvent`s. **Automated** → `crates/tool-runtime` `AuditSink` tests
- [ ] **Path traversal (`../`) blocked by `PathSafetyPolicy`**: Attempts to read/write paths containing `../` are rejected by `PathSafetyPolicy`. **Automated** → `crates/tool-runtime` path safety tests (Phase 13)

---

## Memory / Vault Sync

- [ ] **Canonical docs (SOUL.md, HEARTBEAT.md, etc.) readable**: `VaultAccessor::read` returns correct content for all canonical docs. **Automated** → `crates/memory-sync/tests/integration.rs` → `load_personality_includes_soul_and_identity`
- [ ] **Vault sync handles malformed notes gracefully**: Bad frontmatter and unclosed delimiters return errors, not panics. **Automated** → `malformed_frontmatter_no_colon_via_vault_accessor`, `unclosed_frontmatter_returns_error`
- [ ] **Memory retrieval is token-budget-aware**: Large vault docs are truncated to fit the token budget. **Automated** → `large_soul_doc_is_truncated_within_budget`
- [ ] **Machine sections preserve human prose on sync**: Human-written content in vault files is not overwritten by sync. **Automated** → `update_machine_sections_preserves_human_prose`

---

## Recovery / Resume

- [ ] **Interrupted runs reconciled on restart**: Runs in `Running` or `WaitingForTool` state at crash time are transitioned to `Failed` on next startup. **Automated** → `crates/session-store` reconciliation tests
- [ ] **Session resume works after process restart**: `rustpi session resume <id>` successfully reattaches to a persisted session. **Manual** (requires persistent backend configured)
- [ ] **Partial replay log handled gracefully**: A truncated or corrupt JSONL event log does not crash replay; partial log is replayed up to the last valid record. **Manual** (truncate event log mid-record and run `rustpi replay`)

---

## Known Limitations / Manual Validation Gates

The following items require manual validation with live services and are **not** covered by automated CI:

- **Live OpenAI / Copilot / vLLM / llama.cpp providers**: All provider integration tests require live credentials or local services. CI uses simulation-mode providers only.
- **PostgreSQL backend parity**: Requires a live PostgreSQL instance. Not run in CI.
- **Qdrant semantic memory**: Requires a live Qdrant instance. Upsert and query tests are manual only.
- **Browser-based OAuth flow**: Requires a browser and network access to the provider's auth server.
- **End-to-end crash recovery**: Requires a process kill (`kill -9`) and restart sequence; cannot be fully automated in unit tests.
- **Session resume across restarts**: Depends on a configured persistent backend (SQLite or PostgreSQL file on disk); in-memory stores do not persist across processes.
- **Piped stdin I/O**: Requires a real TTY/pipe; not exercised by process-level binary tests in CI.
- **TUI live rendering**: `TestBackend`-based tests verify state and layout but do not exercise the real `crossterm`/`ratatui` rendering on a physical terminal.

### Known Deferred Features / Intentional Limitations

- **Gemini provider**: Chat, streaming, and embeddings are planned but not yet implemented (Phase roadmap item).
- **llama.cpp embeddings**: Opt-in only; not enabled by default.
- **Vector memory (Qdrant) embeddings via OpenAI-compatible endpoint**: Supported but not validated against all providers.
- **Multi-agent orchestration**: Not in scope for current phases.
- **Windows support**: Untested; `crossterm` should be cross-platform but no CI pipeline for Windows.
