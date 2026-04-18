# rustpi

A native Rust AI agent platform with multi-provider model access, durable sessions, Obsidian-backed local-first memory, and a rich terminal UI.

**Status: Phases 0–4 complete — provider integrations live**

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
| `tool-runtime` | Tool trait, registry, subprocess runner with timeout | 🔧 Phase 5 |
| `context-engine` | Context window assembly, ignore rules, token budgeting | 🔧 Phase 6 |
| `session-store` | Durable session persistence (SQLite / sled / in-memory) | 🔧 Phase 7 |
| `memory-sync` | Obsidian vault integration, vector memory sync | 🔧 Phase 8 |
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
- Qdrant (optional, for vector memory — Phase 7+)
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
| 5 | Tool runtime MVP | 🔲 Planned |
| 6 | Context engine MVP | 🔲 Planned |
| 7 | Session stores and durable memory backends | 🔲 Planned |
| 8 | Obsidian vault memory and personality system | 🔲 Planned |
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

