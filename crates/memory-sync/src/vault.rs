//! Obsidian vault reader/writer.
//!
//! Phase 0 stub.

use std::path::{Path, PathBuf};

/// The mutability classification of a canonical vault document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocMutability {
    /// Never written by the runtime.
    ReadOnly,
    /// May be written by the runtime without approval.
    RuntimeWritable,
    /// Requires user approval before writing.
    ApprovalRequired,
}

/// Canonical vault documents and their mutability rules.
pub const VAULT_DOCS: &[(&str, DocMutability)] = &[
    ("AGENTS.md", DocMutability::ReadOnly),
    ("BOOT.md", DocMutability::ReadOnly),
    ("BOOTSTRAP.md", DocMutability::ReadOnly),
    ("HEARTBEAT.md", DocMutability::RuntimeWritable),
    ("IDENTITY.md", DocMutability::ReadOnly),
    ("SOUL.md", DocMutability::ReadOnly),
    ("TOOLS.md", DocMutability::RuntimeWritable),
    ("USER.md", DocMutability::ApprovalRequired),
];

/// Vault accessor.
///
/// Phase 0 stub — read/write implementation deferred to Phase 8.
pub struct VaultAccessor {
    vault_path: PathBuf,
}

impl VaultAccessor {
    pub fn new(vault_path: impl AsRef<Path>) -> Self {
        Self { vault_path: vault_path.as_ref().to_path_buf() }
    }

    pub fn vault_path(&self) -> &Path {
        &self.vault_path
    }
}
