# rustpi

A native Rust AI agent platform with multi-provider model access, durable sessions, and a local-first memory system.

**Status: Phase 0 — Foundation complete**

---

## Architecture overview

rustpi is organized as a Cargo workspace of 13 focused library crates and two
binary entry points (`cli`, `tui`). The dependency graph flows strictly
downward: primitive types and configuration live in `agent-core` and
`config-core`; higher-level capabilities (model access, tool execution,
sessions, memory) build on top of them; the binary crates wire everything
together. No library crate depends on `cli` or `tui`.

---

## Quick start

```sh
git clone <repo>
cd rustpi
cargo build --workspace
cargo test --workspace
```

---

## Crate overview

| Crate | Role |
|---|---|
| `agent-core` | Shared types, `AgentEvent`, core traits |
| `config-core` | Configuration schema, loading, precedence |
| `auth-core` | Credential storage, OAuth/token flows |
| `event-log` | JSONL-backed durable event log |
| `model-adapters` | Multi-provider LLM client (`ModelProvider` trait) |
| `tool-runtime` | Tool execution engine, sandboxing, policy integration |
| `context-engine` | Context window assembly, ignore rules, token budgeting |
| `session-store` | Durable session persistence (SQLite + in-memory backends) |
| `policy-engine` | Allow/deny/approval rule evaluation |
| `memory-sync` | Local-first memory layer, conflict resolution |
| `rpc-api` | JSONL RPC protocol types and codec |
| `cli` | Command-line binary entry point (Phase 10) |
| `tui` | Terminal UI binary entry point (Phase 11) |

> Full CLI and TUI functionality begins in Phase 10 and Phase 11 respectively.
> Until then, both crates compile but expose minimal behaviour.

---

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md) for build commands, coding
conventions, error handling rules, and the testing strategy.

Architecture decision records live in [`docs/adr/`](./docs/adr/).
