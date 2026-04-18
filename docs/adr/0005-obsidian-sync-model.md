# ADR-0005: Obsidian Vault Sync Model

**Status:** Accepted

## Context

The Obsidian vault is the human-readable memory layer. It must be:
- Readable by the runtime on startup and during runs
- Writable by the runtime for certain documents
- Editable by humans at any time
- Consistent enough that conflicts don't silently corrupt memory

The sync model must work without an Obsidian application running — it operates directly on the Markdown files on disk.

## Decision

The `memory-sync` crate owns all vault I/O through `VaultAccessor`.

**Document mutability is statically defined** in `VAULT_DOCS`:

| Document | Mutability |
|---|---|
| `AGENTS.md` | ReadOnly |
| `BOOT.md` | ReadOnly |
| `BOOTSTRAP.md` | ReadOnly |
| `HEARTBEAT.md` | RuntimeWritable |
| `IDENTITY.md` | ReadOnly |
| `SOUL.md` | ReadOnly |
| `TOOLS.md` | RuntimeWritable |
| `USER.md` | ApprovalRequired |

**Sync rules:**
1. On startup, all vault documents are read and indexed into the retrieval layer.
2. `RuntimeWritable` documents are overwritten by the runtime using atomic rename-to-target to avoid partial writes.
3. `ApprovalRequired` writes are queued and surfaced to the operator before committing.
4. `ReadOnly` documents are never written by the runtime under any circumstance.
5. If a human edits a `RuntimeWritable` document concurrently with a runtime write, the human edit wins (last-writer-wins on the filesystem); the runtime re-reads before its next write.
6. There is no two-phase commit across vault and structured store — eventual consistency is accepted for the vault layer.

**Vault path** is configured in `config-core::MemoryConfig::obsidian_vault_path`. If absent, vault memory is disabled but the runtime continues without it.

## Consequences

**Positive:**
- Simple, auditable file-based sync — no additional sync daemon required
- Human edits are never silently overwritten (except `RuntimeWritable` docs, which are regenerated)
- Vault absence is gracefully tolerated

**Negative:**
- No distributed locking — concurrent access from multiple runtime instances could cause races on `RuntimeWritable` docs
- Eventual consistency means a crash mid-write could leave a document partially updated (atomic rename mitigates this)
- Vault must be on a local or NFS filesystem accessible to the runtime process
