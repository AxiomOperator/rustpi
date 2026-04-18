//! Typed representation of the 8 canonical vault documents.

use std::path::{Path, PathBuf};

use crate::error::MemorySyncError;
use crate::markdown::VaultDoc;

/// Mutability classification of a vault document.
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

/// Which canonical doc is being represented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CanonicalDoc {
    Agents,
    Boot,
    Bootstrap,
    Heartbeat,
    Identity,
    Soul,
    Tools,
    User,
}

impl CanonicalDoc {
    /// All canonical documents in definition order.
    pub fn all() -> &'static [CanonicalDoc] {
        &[
            CanonicalDoc::Soul,
            CanonicalDoc::Identity,
            CanonicalDoc::Agents,
            CanonicalDoc::Boot,
            CanonicalDoc::Bootstrap,
            CanonicalDoc::Heartbeat,
            CanonicalDoc::Tools,
            CanonicalDoc::User,
        ]
    }

    pub fn filename(&self) -> &'static str {
        match self {
            CanonicalDoc::Agents => "AGENTS.md",
            CanonicalDoc::Boot => "BOOT.md",
            CanonicalDoc::Bootstrap => "BOOTSTRAP.md",
            CanonicalDoc::Heartbeat => "HEARTBEAT.md",
            CanonicalDoc::Identity => "IDENTITY.md",
            CanonicalDoc::Soul => "SOUL.md",
            CanonicalDoc::Tools => "TOOLS.md",
            CanonicalDoc::User => "USER.md",
        }
    }

    pub fn mutability(&self) -> DocMutability {
        match self {
            CanonicalDoc::Agents => DocMutability::ReadOnly,
            CanonicalDoc::Boot => DocMutability::ReadOnly,
            CanonicalDoc::Bootstrap => DocMutability::ReadOnly,
            CanonicalDoc::Heartbeat => DocMutability::RuntimeWritable,
            CanonicalDoc::Identity => DocMutability::ReadOnly,
            CanonicalDoc::Soul => DocMutability::ReadOnly,
            CanonicalDoc::Tools => DocMutability::RuntimeWritable,
            CanonicalDoc::User => DocMutability::ApprovalRequired,
        }
    }

    /// Whether this doc is injected into the prompt by default.
    pub fn included_in_prompt(&self) -> bool {
        matches!(
            self,
            CanonicalDoc::Soul
                | CanonicalDoc::Identity
                | CanonicalDoc::Agents
                | CanonicalDoc::Boot
                | CanonicalDoc::User
        )
    }

    /// Prompt assembly priority (0 = highest). Only meaningful for included docs.
    pub fn prompt_priority(&self) -> u8 {
        match self {
            CanonicalDoc::Soul => 0,
            CanonicalDoc::Identity => 1,
            CanonicalDoc::Agents => 2,
            CanonicalDoc::Boot => 3,
            CanonicalDoc::User => 4,
            CanonicalDoc::Bootstrap => 5,
            CanonicalDoc::Heartbeat => 6,
            CanonicalDoc::Tools => 7,
        }
    }

    /// Minimal template for initialising a missing vault document.
    pub fn default_template(&self) -> &'static str {
        match self {
            CanonicalDoc::Soul => "# Soul\n\nCore rules, ethics, and non-negotiables.\n",
            CanonicalDoc::Identity => "# Identity\n\nAgent identity, role, and tone.\n",
            CanonicalDoc::Agents => {
                "# Agents\n\nAgent operating instructions and behavioral conventions.\n"
            }
            CanonicalDoc::Boot => {
                "# Boot\n\nBoot-time essentials and initialization steps.\n"
            }
            CanonicalDoc::Bootstrap => {
                "# Bootstrap\n\nDeep initialization guidance and environment setup.\n"
            }
            CanonicalDoc::Heartbeat => {
                "# Heartbeat\n\n<!-- machine-managed -->\nStatus: idle\n"
            }
            CanonicalDoc::Tools => {
                "# Tools\n\n<!-- machine-managed -->\nNo tools registered yet.\n"
            }
            CanonicalDoc::User => "# User\n\nUser preferences, habits, and communication style.\n",
        }
    }
}

/// Loaded content of a canonical document.
pub struct LoadedDoc {
    pub doc: CanonicalDoc,
    pub parsed: VaultDoc,
    pub path: PathBuf,
}

/// Load a canonical doc from the vault. Returns `Ok(None)` if the file doesn't exist.
pub fn load_doc(
    vault_path: &Path,
    doc: CanonicalDoc,
) -> Result<Option<LoadedDoc>, MemorySyncError> {
    let path = vault_path.join(doc.filename());
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)?;
    let parsed = VaultDoc::parse(&content).map_err(|e| match e {
        MemorySyncError::MalformedMarkdown(_) => {
            MemorySyncError::MalformedMarkdown(doc.filename().to_string())
        }
        other => other,
    })?;
    Ok(Some(LoadedDoc { doc, parsed, path }))
}

/// Load all canonical docs that exist. Missing docs are silently skipped.
pub fn load_all_docs(vault_path: &Path) -> Result<Vec<LoadedDoc>, MemorySyncError> {
    let mut result = Vec::new();
    for &doc in CanonicalDoc::all() {
        if let Some(loaded) = load_doc(vault_path, doc)? {
            result.push(loaded);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_filenames_correct() {
        assert_eq!(CanonicalDoc::Agents.filename(), "AGENTS.md");
        assert_eq!(CanonicalDoc::Boot.filename(), "BOOT.md");
        assert_eq!(CanonicalDoc::Bootstrap.filename(), "BOOTSTRAP.md");
        assert_eq!(CanonicalDoc::Heartbeat.filename(), "HEARTBEAT.md");
        assert_eq!(CanonicalDoc::Identity.filename(), "IDENTITY.md");
        assert_eq!(CanonicalDoc::Soul.filename(), "SOUL.md");
        assert_eq!(CanonicalDoc::Tools.filename(), "TOOLS.md");
        assert_eq!(CanonicalDoc::User.filename(), "USER.md");
    }

    #[test]
    fn prompt_priority_ordering() {
        assert!(CanonicalDoc::Soul.prompt_priority() < CanonicalDoc::Identity.prompt_priority());
        assert!(
            CanonicalDoc::Identity.prompt_priority() < CanonicalDoc::Agents.prompt_priority()
        );
        assert!(CanonicalDoc::Agents.prompt_priority() < CanonicalDoc::Boot.prompt_priority());
    }

    #[test]
    fn load_doc_missing_returns_none() {
        let dir = tempfile::TempDir::new().unwrap();
        let result = load_doc(dir.path(), CanonicalDoc::Soul).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn load_doc_malformed_frontmatter_returns_error() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("SOUL.md");
        std::fs::write(&path, "---\nnot valid yaml line\n---\n").unwrap();
        let result = load_doc(dir.path(), CanonicalDoc::Soul);
        assert!(matches!(result, Err(MemorySyncError::MalformedMarkdown(_))));
    }

    #[test]
    fn load_doc_valid_file_parses() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("SOUL.md");
        std::fs::write(&path, "# Soul\n\nCore ethics.\n").unwrap();
        let loaded = load_doc(dir.path(), CanonicalDoc::Soul).unwrap().unwrap();
        assert_eq!(loaded.doc, CanonicalDoc::Soul);
        assert!(!loaded.parsed.sections.is_empty());
    }
}
