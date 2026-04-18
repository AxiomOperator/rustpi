//! Memory synchronization between the structured store and the Obsidian vault.
//!
//! The vault contains human-readable Markdown memory documents:
//! - `AGENTS.md` — agent identity and capabilities (read-only at runtime)
//! - `BOOT.md` — startup instructions (read-only at runtime)
//! - `BOOTSTRAP.md` — first-run setup notes (read-only at runtime)
//! - `HEARTBEAT.md` — periodic status (writable by runtime)
//! - `IDENTITY.md` — agent identity document (read-only at runtime)
//! - `SOUL.md` — personality and values (read-only at runtime)
//! - `TOOLS.md` — available tool descriptions (writable by runtime)
//! - `USER.md` — user profile and preferences (writable with approval)
//!
//! Phase 0 stub — full sync implementation deferred to Phase 8.

pub mod error;
pub mod vault;
