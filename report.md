# rustpi — Comprehensive Application Readiness Report

**Date:** 2026-04-18  
**Audited by:** Deep code audit across all 14 crates  
**Methodology:** Direct source-file inspection — every finding backed by file:line evidence  

---

## Executive Summary

rustpi is a **well-architected but not yet functional** AI agent platform. It has 14 production-quality crates implementing state machines, storage backends, authentication, HTTP adapters, a terminal UI, CLI, RPC protocol, context engine, vault sync, security controls, and observability. The code is genuinely impressive at the component level.

**The central problem is simple: none of the components are connected.**

When a user runs `rustpi run "write me a poem"` today, the output is:

```
token chunk 0token chunk 1token chunk 2
```

The prompt is silently discarded. No model is called. No context is assembled. No tools run. The system returns three hardcoded strings and exits cleanly with code 0.

This is not a bug — it is an intentional stub left in `rpc-api/src/dispatch.rs` that was never replaced with real model invocation. Every surrounding component (the providers, the config system, the context engine, the tools, the auth layer) is real and works correctly in isolation. They simply have no caller.

**Estimated gap to a minimally functional application: 2–4 weeks of integration work**, not greenfield development. The infrastructure exists. The plumbing does not.

---

## Component Reality Map

This table summarises every crate's actual implementation status, assessed by reading source code — not documentation.

| Crate | Implementation | Tests | Wired to Runtime | Notes |
|---|---|---|---|---|
| **agent-core** | ✅ Real | ✅ 55 tests | ⚠️ Partial | State machines real; SessionManager in-memory only; no model call |
| **auth-core** | ✅ Real | ✅ 40 tests | ❌ No | AES-256-GCM, OAuth2/PKCE, device flow, refresh — all real; never called |
| **cli** | ✅ Real | ✅ 27 tests | ⚠️ Partial | Routing, streaming display real; auth commands stub; submits to stub dispatcher |
| **config-core** | ✅ Real | ✅ 10 tests | ⚠️ Partial | Reads TOML + env, merges layers; config loaded but never used to build providers |
| **context-engine** | ✅ Real | ✅ 30 tests | ❌ No | Scanner, ignore, relevance, packer, compactor all real; never called from runtime |
| **event-log** | ✅ Real | ✅ 43 tests | ❌ No | JSONL append log real; never written to by the runtime |
| **memory-sync** | ✅ Real | ✅ 38 tests | ❌ No | VaultAccessor, SyncEngine, personality injection real; never called |
| **model-adapters** | ✅ Real | ✅ 77 tests | ❌ No | OpenAI, vLLM, llama.cpp, Copilot — full reqwest/SSE implementations; never instantiated |
| **observability** | ✅ Real | ✅ 27 tests | ❌ No | Metrics collector real; never subscribed to event bus |
| **policy-engine** | ✅ Real | ✅ 27 tests | ❌ No | Glob rule evaluation real; never consulted |
| **rpc-api** | ⚠️ Partial | ✅ 70 tests | ✅ Yes | Protocol and transport real; model dispatch is hardcoded stub |
| **session-store** | ✅ Real | ✅ 20 tests | ❌ No | SQLite, sled, PostgreSQL all real; runtime never writes to them |
| **tool-runtime** | ✅ Real | ✅ 70 tests | ❌ No | Shell, file, edit tools real; ToolRunner never called from dispatch |
| **tui** | ⚠️ Partial | ✅ 40 tests | ⚠️ Partial | Renders events correctly; submit input does nothing (no run_start call) |

---

## Critical Gap: The Dispatch Stub

All missing functionality converges on one location: **`crates/rpc-api/src/dispatch.rs`, lines 261–307**.

This is the `handle_run_start()` function — the handler called when `rustpi run` submits a prompt. Its current implementation:

```rust
// Line 222: prompt is received but immediately suppressed
let _prompt = prompt.to_string();  // leading underscore = intentionally unused

// Lines 261–307: "run" spawns a background task that:
tokio::spawn(async move {
    for i in 0..3usize {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let event = AgentEvent::TokenChunk {
            delta: format!("token chunk {}", i),  // hardcoded fake output
            ...
        };
        state_clone.event_bus.emit(event);
    }
    // then emits RunCompleted and exits
});
```

The `provider_id` and `model` parameters are also accepted but ignored. No provider is looked up, no HTTP request is made, no context is assembled.

Every other gap described in this report is a downstream consequence of this one missing integration.

---

## What Needs to Be Built

The following sections describe each integration gap in priority order for achieving a functioning application.

---

### Gap 1 — Provider Registry and Instantiation [CRITICAL]

**What's missing:** No code builds a `ModelProvider` instance from configuration.

**What exists:**
- `config-core` reads `[[providers]]` entries from `~/.config/rustpi/config.toml` into `Vec<ProviderConfig>` ✅
- `model-adapters` has fully working `OpenAiAdapter`, `LlamaCppAdapter`, `VllmAdapter`, `CopilotAdapter` ✅
- A `ProviderRegistry` struct exists in `model-adapters/src/provider.rs` ✅

**What's absent:** The code that reads `config.providers`, selects the right adapter type (based on `ProviderConfig.kind`), and registers it in the `ProviderRegistry`. This factory/bootstrap step does not exist anywhere.

**Where to add it:** In `cli/src/main.rs` or `rpc-api/src/server.rs` during startup, before the server loop begins.

**Approximate scope:** ~80–120 lines — a `build_provider_registry(config: &Config) -> ProviderRegistry` function.

---

### Gap 2 — Auth Credential Resolution [CRITICAL]

**What's missing:** No code resolves authentication credentials and injects them into provider adapters.

**What exists:**
- `auth-core` has `ApiKeyAuth::resolve_key()` (reads from env var or config file) ✅
- `auth-core` has `OAuthFlow`, `DeviceFlow`, `EncryptedFileTokenStore` ✅
- `config-core` has `ProviderAuthConfig` enum: `ApiKey { env_var }`, `OAuthBrowser`, `DeviceCode` ✅

**What's absent:** The code that takes a `ProviderConfig.auth` field and resolves it to an actual credential, then passes it to the adapter constructor. The RPC `auth_status` handler always returns `authenticated: false` regardless of actual stored tokens. The CLI `auth login` command explicitly returns an error saying login is not implemented.

**Where to add it:** In the provider registry builder (Gap 1), after looking up the adapter type, call auth-core to resolve credentials and pass them to the adapter config.

**Approximate scope:** ~100–150 lines in the bootstrap function plus ~50 lines updating the auth_status handler.

---

### Gap 3 — Real Model Invocation in Dispatch [CRITICAL]

**What's missing:** `handle_run_start()` must be rewritten to call a real provider instead of emitting hardcoded tokens.

**What needs to happen:**
1. Look up the requested provider from the `ProviderRegistry` (Gap 1)
2. Build a `CompletionRequest` from the prompt, model, and any context (Gap 5)
3. Call `provider.complete_stream(request)` to get a `Stream<TokenDelta>`
4. For each `TokenDelta`, emit `AgentEvent::TokenChunk` on the event bus
5. On stream end, emit `AgentEvent::RunCompleted`
6. On error, emit `AgentEvent::RunFailed`

**Approximate scope:** ~100–150 lines replacing the current stub. The streaming fan-out infrastructure (event bus, RPC transport, CLI display) already works and requires no changes.

---

### Gap 4 — Session and Event Persistence [HIGH]

**What's missing:** Sessions, runs, and events are stored only in-memory and are lost on process exit.

**What exists:**
- `session-store` has fully working `SqliteBackend`, `SledBackend`, `PostgresBackend` ✅
- `session-store` has a `factory.rs` that can build the right backend from config ✅
- `event-log` has a working `FileEventStore` for JSONL append-on-disk logging ✅

**What's absent:** The `ServerState` struct in `rpc-api/src/server.rs` holds sessions and runs in `HashMap`s with no backing store. Nothing calls `session_store::factory::build_session_store()` to initialize a persistent backend at startup. Nothing calls `event_log::FileEventStore::append()` when events are emitted.

**Where to add it:** 
- Add `SessionStore`, `RunStore`, `EventStore` trait objects to `ServerState`
- Initialize them from config in `cli/src/main.rs` using the factory
- In `handle_run_start()` and related handlers, persist state changes and events

**Approximate scope:** ~200–300 lines across server.rs, dispatch.rs, and startup code.

---

### Gap 5 — Context Engine Integration [HIGH]

**What's missing:** No code calls the context engine to assemble file context for prompts.

**What exists:**
- `context-engine` has a fully working `ContextEngine::build_context()` pipeline ✅
- Scanner, ignore rules (.gitignore + .contextignore), relevance scoring, token-budgeted packer — all real ✅
- `context-engine` is listed as a **dev-dependency only** in `agent-core/Cargo.toml` (never as a production dependency anywhere)

**What's absent:** Any call to `ContextEngine` from the dispatch layer, CLI executor, or agent-core. The context engine works when tested in isolation but has zero callers in production paths.

**Where to add it:** In `handle_run_start()`, before calling the model provider, call `ContextEngine::build_context()` to assemble project-relevant context and include file excerpts in the `CompletionRequest.messages`.

**Approximate scope:** ~60–80 lines plus adding `context-engine` as a production dependency in `rpc-api/Cargo.toml` or `cli/Cargo.toml`.

---

### Gap 6 — Memory and Personality Injection [HIGH]

**What's missing:** No code loads vault personality documents or injects them into prompts.

**What exists:**
- `memory-sync` has `load_personality()` and `inject_personality()` which wire correctly into `PromptAssembler.system()` ✅
- `VaultAccessor` does real filesystem I/O to Obsidian vault directories ✅
- `SyncEngine` syncs structured store → vault and can index vault contents ✅
- `memory-sync` is **not listed as a dependency** in `rpc-api/Cargo.toml`, `cli/Cargo.toml`, or `agent-core/Cargo.toml`

**What's absent:** Any caller of `load_personality()` or `inject_personality()` from the runtime. The vault path would come from config (needs a `vault_path` config field or env var).

**Where to add it:** In the dispatch layer, load personality context from the configured vault path before building the completion request.

**Approximate scope:** ~40–60 lines plus adding `memory-sync` as a production dependency and a vault config field.

---

### Gap 7 — Tool Execution [HIGH]

**What's missing:** When a model returns a tool call, nothing executes the tool.

**What exists:**
- `tool-runtime` has fully working `ToolRunner`, `ShellTool`, `ReadFileTool`, `WriteFileTool`, `EditTool` ✅
- `ToolRunner.execute()` handles approval, timeout, cancellation, event emission ✅
- `agent-core` has `ToolOrchestrator` which tracks tool call state ✅
- `tool-runtime` is **not imported** anywhere in `rpc-api/src/dispatch.rs`

**What's absent:** The code that:
1. Parses tool calls from model responses (OpenAI-format `tool_calls` in completions)
2. Routes them to `ToolRunner.execute()` with appropriate approval hook
3. Returns tool results back to the model for the next completion round

**Where to add it:** In `handle_run_start()`, after streaming initial response, check for tool calls in the final message and process them in a loop until the model produces a non-tool-call response.

**Approximate scope:** ~150–200 lines for the tool-call loop, plus an approval hook that can prompt the user over the event bus.

---

### Gap 8 — TUI Input Submission [MEDIUM]

**What's missing:** Typing a message in the TUI and pressing Enter does not trigger a run.

**What exists:**
- `tui/src/app.rs` handles `KeyAction::SubmitInput` (line ~146)
- The input buffer is cleared and the message is added to the local message list
- A status message is set: `"(prompt submitted — no model connected)"`
- The TUI is fully wired to the event bus and correctly renders all event types ✅

**What's absent:** The `SubmitInput` handler needs to call `executor.run_start(session_id, prompt)` — the same call the CLI makes. The executor and server state are available in the `App` struct.

**Approximate scope:** ~20–30 lines in `app.rs`.

---

### Gap 9 — Policy Engine Enforcement [MEDIUM]

**What's missing:** No policy rules are evaluated before tool execution or provider access.

**What exists:**
- `policy-engine` has a working `PolicyEngine` with glob-based first-match-wins rules ✅
- `policy-engine` is present in `Cargo.toml` but has zero callers in `tool-runtime` or `rpc-api`

**What's absent:** Calls to `policy_engine.evaluate_tool()` before `ToolRunner.execute()` and `policy_engine.evaluate_provider()` before model calls.

**Approximate scope:** ~30–50 lines adding policy checks at the two enforcement points.

---

### Gap 10 — Observability Subscription [LOW]

**What's missing:** Metrics are never collected because `TelemetryCollector` is never subscribed to the event bus.

**What exists:**
- `observability` has a real `TelemetryCollector` that processes `AgentEvent`s into metrics ✅
- The event bus in `ServerState` broadcasts all events
- `TelemetryCollector::subscribe_to_bus()` exists and works ✅

**What's absent:** One line at server startup: `collector.subscribe_to_bus(&state.event_bus)`.

**Approximate scope:** ~10 lines.

---

### Gap 11 — Missing Features [LOWER PRIORITY]

These items were identified during audit as entirely absent (not stubbed):

| Feature | Status | Notes |
|---|---|---|
| Gemini adapter | ❌ Missing | `ProviderKind::Gemini` exists in config model, no adapter crate |
| BOOTSTRAP.md canonical doc | ❌ Missing | Defined in `docs.rs` enum but template and tests absent |
| OTLP/Prometheus export | ❌ Stub | Metrics collected in-memory; no gRPC/HTTP export configured |
| CLI auth login flow | ❌ Explicit stub | Returns `"login_not_implemented"` error; no device flow invocation |
| Crash recovery auto-trigger | ❌ Manual only | `RecoveryScanner` must be explicitly called; not hooked to startup |
| Startup session reload | ❌ Missing | Sessions load from in-memory map on startup; no DB read |

---

## Integration Dependency Order

The gaps are not independent. The recommended implementation order is:

```
Gap 1: Provider Registry Bootstrap
    ↓
Gap 2: Auth Credential Resolution
    ↓
Gap 3: Real Model Invocation  ← CORE LOOP
    ↓
Gap 4: Persistence            Gap 5: Context Engine    Gap 6: Memory/Personality
    ↓                              ↓                         ↓
                            Gap 7: Tool Execution
                                    ↓
                  Gap 8: TUI Input    Gap 9: Policy    Gap 10: Observability
```

A minimal end-to-end demo (user types prompt → real LLM responds) requires only Gaps 1–3. The rest add production quality, persistence, and full feature parity.

---

## What Works Today

To be clear about current capabilities:

| Capability | Works Today |
|---|---|
| `rustpi --help` and flag parsing | ✅ Yes |
| `rustpi run "prompt"` | ✅ Returns 3 fake token chunks |
| `rustpi session list` | ✅ Lists in-memory sessions |
| `rustpi auth status` | ✅ Always returns "not authenticated" |
| `rustpi auth login` | ❌ Returns explicit error |
| `rustpi diag` | ✅ Returns config/system info |
| TUI rendering and navigation | ✅ Yes |
| TUI chat input submission | ❌ Does nothing |
| Any real LLM call | ❌ No |
| Tool execution | ❌ No |
| Session persistence across restarts | ❌ No |
| Vault/memory sync | ❌ No |
| Policy enforcement | ❌ No |
| Observability metrics | ❌ No (not subscribed) |

---

## Test Coverage vs. Real Wiring

The project has **652 passing tests** across all 14 crates. This is real test coverage of real component behaviour. However, the tests largely validate components in **isolation**. The integration tests (e.g., `lifecycle_integration.rs`) prove that individual components work, but they do not prove end-to-end system behaviour because there is no wired system to test.

This is not a criticism of the tests — they are well-written and catch real behaviour. It is a structural observation: isolated component tests cannot detect wiring gaps, which is precisely what this audit was designed to find.

---

## Effort Estimate

| Gap | Estimated Lines | Estimated Days | Depends On |
|---|---|---|---|
| Gap 1: Provider Registry | ~120 | 1 | — |
| Gap 2: Auth Resolution | ~150 | 1–2 | Gap 1 |
| Gap 3: Real Model Invocation | ~150 | 1–2 | Gaps 1, 2 |
| Gap 4: Persistence | ~300 | 2–3 | — (parallel) |
| Gap 5: Context Engine | ~80 | 0.5–1 | Gap 3 |
| Gap 6: Memory/Personality | ~60 | 0.5 | Gap 3 |
| Gap 7: Tool Execution | ~200 | 2 | Gap 3 |
| Gap 8: TUI Input | ~30 | 0.5 | Gap 3 |
| Gap 9: Policy Enforcement | ~50 | 0.5 | Gap 7 |
| Gap 10: Observability | ~10 | 0.5 | Gap 3 |
| **Total** | **~1,150** | **~10–13 days** | |

A minimal demo (real LLM response in CLI) can be achieved in approximately **3–5 days** by addressing Gaps 1–3 only.

---

## Conclusion

rustpi contains more than 14,000 lines of well-written, well-tested Rust infrastructure. The architecture is sound. The individual components — HTTP adapters, auth flows, storage backends, security controls, the context engine, vault sync, the TUI, CLI — are all real and production-quality.

The project is not a functioning agent application because **the integration layer was never written**. The single entry point that should connect all of these components — `handle_run_start()` in `rpc-api/src/dispatch.rs` — emits three hardcoded strings instead.

The work remaining is **integration engineering**, not architecture design or component implementation. The components are built; they need to be connected.
