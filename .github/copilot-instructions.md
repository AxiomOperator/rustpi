# Copilot Instructions

## Project Overview

`rustpi` is a native Rust AI agent platform — a greenfield monorepo in early Phase 0 (foundation). The planning document is `project.md`.

## Workspace Structure

This is a Cargo workspace. When crates are created, they follow this boundary plan:

| Crate | Responsibility |
|---|---|
| `agent-core` | Session/run state machine, event bus, prompt assembly, cancellation |
| `model-adapters` | Provider abstraction trait + per-provider implementations |
| `tool-runtime` | Subprocess execution, file/search tools, approval hooks, timeout |
| `context-engine` | Filesystem scanner, ignore rules, relevance scoring, context packing |
| `session-store` | Persistent session/run/event storage (SQLite, sled, PostgreSQL backends) |
| `rpc-api` | stdin/stdout JSONL RPC protocol for machine-to-machine embedding |
| `cli` | Scriptable CLI (print mode, JSON mode, file-task mode, auth commands) |
| `tui` | Ratatui full-screen interactive operator interface |
| `auth-core` | OAuth browser flow, device authorization flow, API key storage, token refresh |
| `event-log` | Append-only structured event storage and replay reader |
| `config-core` | Layered config: global → project → user → provider → memory → interface |
| `memory-sync` | Structured store ↔ Obsidian vault sync, conflict resolution |
| `policy-engine` | Tool allow/deny rules, file mutation policy, provider selection rules |

## Build & Test Commands

```sh
# Build the full workspace
cargo build

# Build in release mode
cargo build --release

# Run all tests
cargo test

# Run tests for a single crate
cargo test -p agent-core

# Run a single test by name
cargo test -p agent-core test_name

# Check without building
cargo check

# Lint
cargo clippy -- -D warnings

# Format
cargo fmt
cargo fmt --check   # CI check (no writes)
```

## Core Architecture Decisions

### Crate Dependency Rules
Crates lower in the stack must not depend on crates higher in the stack. The dependency direction flows: `agent-core` / `config-core` / `policy-engine` → `model-adapters` / `tool-runtime` / `context-engine` / `session-store` / `event-log` → `auth-core` → `rpc-api` → `cli` / `tui`.

### Serialization
- **JSON** for configuration and structured data
- **JSONL** (newline-delimited JSON) for streaming events and the RPC protocol

### Shared Core Types
These types live in `agent-core` and are used workspace-wide:
`RunId`, `SessionId`, `ToolCall`, `ToolResult`, `ProviderId`, `ModelId`, `AuthState`, `AgentEvent`

### Event Model
The runtime is append-only and event-driven. All major lifecycle transitions (session, run, tool execution, auth state changes) emit structured `AgentEvent` variants. Events must be serializable for replay and audit.

### Provider Abstraction
All model providers implement a unified internal trait covering: chat/completions, streaming, embeddings, model discovery, tool-calling capability flags, and auth state inspection. Provider errors must be normalized into a shared internal schema.

### Auth Model
`auth-core` is shared by all providers. Supports three auth flows: OAuth browser login, device authorization flow (with device code/user code display), and API key. Tokens are encrypted at rest and refresh automatically.

### Memory Layering
Two memory layers that must stay in sync via `memory-sync`:
1. **Structured stores** — SQLite / sled / PostgreSQL / Qdrant (vector) for runtime use
2. **Obsidian vault** — human-readable Markdown memory at a configurable vault path

Canonical vault documents: `AGENTS.md`, `BOOT.md`, `BOOTSTRAP.md`, `HEARTBEAT.md`, `IDENTITY.md`, `SOUL.md`, `TOOLS.md`, `USER.md`. Each has a defined mutability (read-only at runtime / writable by runtime / writable only with approval).

### Tool Runtime Safety
All tools execute through a unified runtime with: timeout enforcement, cancellation, approval hooks for sensitive actions, path traversal protection, and per-event streaming (started / stdout / stderr / completed / cancelled / failed).

### RPC Protocol
The `rpc-api` crate exposes a stdin/stdout JSONL protocol. The CLI and future editor integrations drive the runtime through this interface. Session attach/detach, run start/stop, streaming events, auth-status queries, and capability discovery are all RPC operations.

## Key Conventions

### Error Handling
Define error types per crate using `thiserror`. Provider errors normalize into a shared internal schema before crossing crate boundaries.

### Logging and Tracing
Use `tracing` for structured instrumentation. Secrets must be redacted before any log/event emission.

### Testing Strategy
- Unit tests per crate (in `src/` using `#[cfg(test)]`)
- Integration tests in `tests/` per crate for cross-boundary behavior
- Provider adapters test against mock/provider test harnesses
- Storage backends have a parity test suite run against all backends
- Failure/chaos tests cover: token expiry, network drop, hung tool, corrupted note, partial replay log

### Config Precedence
`global < project < user < provider < memory < CLI/TUI`. Higher layers override lower ones.
