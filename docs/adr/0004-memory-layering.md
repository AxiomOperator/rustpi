# ADR-0004: Memory Layering

**Status:** Accepted

## Context

The agent needs multiple memory layers with different characteristics:
- **Working memory:** in-process state for the current run (ephemeral)
- **Session memory:** structured persistence across runs (SQLite/sled/PostgreSQL)
- **Vector memory:** semantic retrieval via embeddings (Qdrant)
- **Human-readable memory:** auditable, editable long-term notes (Obsidian vault)

These layers must coexist, stay in sync where appropriate, and be queryable from prompt assembly.

## Decision

Adopt a **four-layer memory architecture**:

| Layer | Crate | Backend | Persistence |
|---|---|---|---|
| Working | `agent-core` | In-process `HashMap` | Run-scoped, ephemeral |
| Structured | `session-store` | SQLite / sled / PostgreSQL | Durable, queryable |
| Vector | `session-store` | Qdrant | Durable, semantic |
| Vault | `memory-sync` | Obsidian Markdown files | Durable, human-readable |

Key decisions:
1. **Backends are trait-gated.** `SessionStore` and `RunStore` traits in `session-store` allow swapping SQLite for PostgreSQL without changing callers.
2. **Vault documents have explicit mutability rules.** `AGENTS.md`, `BOOT.md`, `BOOTSTRAP.md`, `IDENTITY.md`, `SOUL.md` are read-only at runtime. `HEARTBEAT.md`, `TOOLS.md` are runtime-writable. `USER.md` requires user approval.
3. **`memory-sync` owns vault ↔ structured store synchronization.** Conflict resolution favors the vault for human-edited documents; the structured store wins for runtime-generated summaries.
4. **Vector memory is optional.** If Qdrant is not configured, semantic retrieval degrades gracefully to recency-based structured retrieval.
5. **Context assembly (`context-engine`) pulls from all layers** at prompt-build time, respecting a token budget.

## Consequences

**Positive:**
- Human operators can audit and edit long-term memory directly in the vault
- Vector retrieval enables semantic memory without exposing Qdrant details to other crates
- Backend swapping supports deployment from laptop (SQLite) to production (PostgreSQL)

**Negative:**
- Four-layer sync adds complexity; conflicts must be handled explicitly
- Obsidian vault assumes a local filesystem path, limiting cloud-only deployments
- Qdrant requires a running service; startup must tolerate its absence gracefully
