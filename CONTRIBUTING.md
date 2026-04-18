# Contributing to rustpi

Thank you for contributing to rustpi! This guide covers everything you need to
build, test, and extend the codebase consistently.

---

## Prerequisites

- **Rust stable toolchain** — install via [rustup](https://rustup.rs/)
  ```sh
  rustup toolchain install stable
  rustup component add clippy rustfmt
  ```
- **cargo** is included with the toolchain; no nightly required at this time.

---

## Build commands

| Command | Purpose |
|---|---|
| `cargo build --workspace` | Build all crates |
| `cargo test --workspace` | Run all tests across the workspace |
| `cargo test -p <crate-name>` | Run tests for a single crate |
| `cargo test -p <crate-name> <test_name>` | Run a single test by name |
| `cargo clippy --workspace --all-targets -- -D warnings` | Lint (CI-enforced) |
| `cargo fmt --all` | Format all source files |
| `cargo fmt --all --check` | Check formatting without modifying files (used by CI) |

---

## Crate dependency rules

The workspace is structured as a strict one-way dependency graph. Crates at the
bottom of the stack are pure library primitives; crates at the top are binary
entry points.

```
cli / tui                        ← binary entry points (top)
    └── rpc-api
    └── context-engine
    └── tool-runtime
    └── model-adapters
    └── memory-sync
    └── session-store
    └── policy-engine
    └── event-log
    └── auth-core
    └── config-core
    └── agent-core               ← core primitives (bottom)
```

**Rules:**
- Dependencies flow **downward only** — no cycles, no upward imports.
- **No crate may depend on `cli` or `tui`.** Those two are leaf binaries;
  anything that needs to be shared must live in a library crate below them.
- When introducing a dependency between two existing crates, verify it does not
  create a cycle (`cargo metadata --format-version 1 | jq '.resolve'`).

---

## Error handling conventions

### Library crates (`crates/*` except `cli` and `tui`)

- Define exactly **one `Error` enum per crate** in `src/error.rs`.
- Derive it with [`thiserror`](https://docs.rs/thiserror):
  ```rust
  // src/error.rs
  use thiserror::Error;

  #[derive(Debug, Error)]
  pub enum Error {
      #[error("session not found: {id}")]
      NotFound { id: String },
      #[error("serialization failed")]
      Serialize(#[from] serde_json::Error),
  }
  ```
- Re-export as `pub use error::Error;` from `lib.rs` and expose a `Result<T>`
  alias: `pub type Result<T> = std::result::Result<T, Error>;`
- **Translate foreign errors at crate boundaries.** Callers must not handle
  downstream crate internals. Map errors explicitly with `map_err` or `#[from]`
  rather than leaking them raw.
- **Never use `Box<dyn Error>` across public APIs.** Concrete error types allow
  callers to match and handle specific variants.
- **Never call `.unwrap()` in library code.** Use `?` for propagation, or
  handle the error explicitly with a `match`/`if let`.

### Binary entry points (`cli`, `tui`)

- Use [`anyhow`](https://docs.rs/anyhow) for ergonomic top-level error
  propagation:
  ```rust
  fn main() -> anyhow::Result<()> { ... }
  ```
- `anyhow::Context::context` / `.with_context` are preferred over bare `?` to
  give operators meaningful error messages.

---

## Logging and tracing standards

All crates instrument with the [`tracing`](https://docs.rs/tracing) crate.

### Initialization

`tracing-subscriber` is set up **once**, in the binary entry points (`cli`,
`tui`). Library crates must never call `tracing_subscriber::init()` or any
global subscriber setup.

### Structured fields

Always use structured key-value fields — avoid string interpolation inside the
message:

```rust
// ✅ correct
tracing::info!(session_id = %id, model = %provider, "session started");

// ❌ avoid
tracing::info!("session {} started with model {}", id, provider);
```

### Security — never log secrets

**Never log token values, API keys, auth credentials, or any value that could
constitute a secret.** Redact before any tracing call:

```rust
tracing::debug!(token = "<redacted>", "token refreshed");
```

### Log levels

| Level | When to use |
|---|---|
| `error` | Unrecoverable failures that require operator attention |
| `warn` | Recoverable issues the system handled but the operator should know about |
| `info` | Lifecycle events (session start/end, config loaded, provider selected) |
| `debug` | Request/response details, state transitions |
| `trace` | Hot-path internals, tight loop diagnostics |

### Events vs logs

Runtime events (`AgentEvent`) are **distinct from log entries**. Events flow
through the event bus and are persisted by `event-log`. Do not use `tracing` as
a substitute for the event bus.

---

## Serialization standards

- **JSON** for all structured payloads: config files, RPC messages, stored
  records.
- **JSONL** (newline-delimited JSON) for streaming events, event log files, and
  the RPC protocol.
- All enums that are stored or streamed must use the internal tag format:
  ```rust
  #[derive(Serialize, Deserialize)]
  #[serde(tag = "type", rename_all = "snake_case")]
  pub enum AgentEvent { ... }
  ```
- All public structs/enums that cross serialization boundaries must use
  snake_case field names:
  ```rust
  #[derive(Serialize, Deserialize)]
  #[serde(rename_all = "snake_case")]
  pub struct SessionRecord { ... }
  ```
- Prefer `serde_json::to_writer` / `from_reader` over `to_string`/`from_str`
  when writing to or reading from IO to avoid unnecessary allocations.

---

## Testing strategy by crate

| Crate | Unit tests | Integration tests | Notes |
|---|---|---|---|
| `agent-core` | ✅ Required | — | Types, event roundtrip, error paths |
| `auth-core` | ✅ Required | Phase 3 | Mock token store, flow state machines |
| `config-core` | ✅ Required | — | Precedence rules, parse correctness |
| `event-log` | ✅ Required | — | JSONL encode/decode roundtrip, corrupt-line handling |
| `model-adapters` | ✅ Required | Phase 4 (mock HTTP) | Provider trait contracts, error normalization |
| `tool-runtime` | ✅ Required | Phase 5 | Timeout, cancellation, policy denial |
| `context-engine` | ✅ Required | Phase 6 | Ignore rules, token budgeting |
| `session-store` | ✅ Required | Phase 7 (per backend) | Backend parity tests |
| `policy-engine` | ✅ Required | — | Rule evaluation, allow/deny/approval |
| `memory-sync` | ✅ Required | Phase 8 | Mutability rules, conflict handling |
| `rpc-api` | ✅ Required | Phase 9 (stub host) | Protocol roundtrip |
| `cli` | E2E tests | Phase 10 | Integration against the full runtime |
| `tui` | Snapshot tests | Phase 11 | Ratatui widget rendering |

### General rules

- **Unit tests** live in `src/` under `#[cfg(test)]` modules in the relevant
  source file.
- **Integration tests** live in `tests/` within the crate directory.
- Prefer **real types over mocks**; mock only at external boundaries (HTTP,
  filesystem, OS keychain, system clock).
- Tests must **not require network access** unless explicitly tagged `#[ignore]`
  with a comment explaining the requirement.
- Cover **failure paths** (errors, timeouts, invalid input) not just the happy
  path.

---

## Commit conventions

We recommend [Conventional Commits](https://www.conventionalcommits.org/).
Use the affected crate name as the scope:

```
feat(agent-core): add run cancellation
fix(session-store): handle concurrent write on SQLite backend
docs(contributing): clarify error handling conventions
chore(deps): bump serde to 1.0.200
test(policy-engine): add deny-all rule coverage
refactor(model-adapters): extract retry logic into shared helper
```

| Type | When to use |
|---|---|
| `feat` | New user-facing functionality |
| `fix` | Bug fixes |
| `docs` | Documentation only |
| `chore` | Build, dependency, or tooling changes |
| `test` | Adding or fixing tests without changing production code |
| `refactor` | Code restructuring without behaviour change |

Breaking changes append `!` after the type/scope (e.g. `feat(rpc-api)!:`) and
include a `BREAKING CHANGE:` footer in the commit body.

---

## Adding a new provider adapter

1. **Implement the provider traits** in `crates/model-adapters/src/providers/`:
   - `ModelProvider` — streaming and non-streaming completion
   - `ProviderAuth` — credential resolution and token refresh
2. **Add a `ProviderKind` variant** in `crates/config-core/src/` to allow the
   new provider to be selected via configuration.
3. **Register the provider** in the `ProviderRegistry` inside
   `crates/model-adapters` (Phase 4 implementation task).
4. Add unit tests covering at minimum: successful completion, auth failure,
   rate-limit error normalization, and network timeout.
5. Document the new provider's required config keys in `docs/` and, if
   applicable, in a new ADR under `docs/adr/`.
