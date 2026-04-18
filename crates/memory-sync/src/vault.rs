//! Obsidian vault reader/writer.

use std::path::{Path, PathBuf};

use crate::docs::{CanonicalDoc, DocMutability};
use crate::error::MemorySyncError;
use crate::markdown::VaultDoc;

/// Vault accessor with path-safety guarantees.
pub struct VaultAccessor {
    vault_path: PathBuf,
}

impl VaultAccessor {
    /// Open a vault at `vault_path`. Returns an error if the path doesn't
    /// exist or is not a directory.
    pub fn open(vault_path: impl AsRef<Path>) -> Result<Self, MemorySyncError> {
        let path = vault_path.as_ref().to_path_buf();
        if !path.exists() {
            return Err(MemorySyncError::VaultNotFound(path.display().to_string()));
        }
        if !path.is_dir() {
            return Err(MemorySyncError::VaultNotFound(format!(
                "{} is not a directory",
                path.display()
            )));
        }
        Ok(Self { vault_path: path })
    }

    pub fn vault_path(&self) -> &Path {
        &self.vault_path
    }

    /// Read a canonical doc. Returns `Ok(None)` if the file is absent.
    pub fn read_doc(&self, doc: CanonicalDoc) -> Result<Option<VaultDoc>, MemorySyncError> {
        let path = self.vault_path.join(doc.filename());
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
        Ok(Some(parsed))
    }

    /// Write a canonical doc, enforcing the mutability policy.
    ///
    /// - `ReadOnly` → `MemorySyncError::ReadOnly`
    /// - `ApprovalRequired` → `MemorySyncError::ApprovalRequired`
    /// - `RuntimeWritable` → writes the file
    pub fn write_doc(
        &self,
        doc: CanonicalDoc,
        content: &VaultDoc,
    ) -> Result<(), MemorySyncError> {
        match doc.mutability() {
            DocMutability::ReadOnly => {
                Err(MemorySyncError::ReadOnly(doc.filename().to_string()))
            }
            DocMutability::ApprovalRequired => {
                Err(MemorySyncError::ApprovalRequired(doc.filename().to_string()))
            }
            DocMutability::RuntimeWritable => self.write_raw(doc, content),
        }
    }

    /// Write a canonical doc, bypassing the approval requirement (used when
    /// approval has already been granted externally).  ReadOnly is still
    /// enforced.
    pub fn write_doc_approved(
        &self,
        doc: CanonicalDoc,
        content: &VaultDoc,
    ) -> Result<(), MemorySyncError> {
        if doc.mutability() == DocMutability::ReadOnly {
            return Err(MemorySyncError::ReadOnly(doc.filename().to_string()));
        }
        self.write_raw(doc, content)
    }

    /// Update only the machine-managed sections in a doc, preserving all
    /// human-authored sections.  Creates the file from its default template
    /// if it doesn't exist yet.
    pub fn update_machine_sections(
        &self,
        doc: CanonicalDoc,
        sections: &[(&str, &str)],
    ) -> Result<(), MemorySyncError> {
        let mut vault_doc = match self.read_doc(doc)? {
            Some(d) => d,
            None => VaultDoc::parse(doc.default_template()).map_err(|_| {
                MemorySyncError::Init(format!("bad default template for {}", doc.filename()))
            })?,
        };
        for (heading, body) in sections {
            vault_doc.upsert_machine_section(heading, body);
        }
        self.write_doc_approved(doc, &vault_doc)
    }

    /// Read any arbitrary file in the vault by relative path.
    pub fn read_file(&self, relative_path: &Path) -> Result<Option<String>, MemorySyncError> {
        check_no_traversal(relative_path)?;
        let full = self.vault_path.join(relative_path);
        if !full.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&full)?;
        Ok(Some(content))
    }

    /// List all Markdown files (*.md) in the vault root (non-recursive).
    pub fn list_files(&self) -> Result<Vec<PathBuf>, MemorySyncError> {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(&self.vault_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file()
                && path.extension().and_then(|e| e.to_str()) == Some("md")
            {
                files.push(path);
            }
        }
        files.sort();
        Ok(files)
    }

    /// Initialise the vault by creating default templates for any missing
    /// canonical docs.  Returns the list of docs that were created.
    pub fn init_defaults(&self) -> Result<Vec<CanonicalDoc>, MemorySyncError> {
        let mut created = Vec::new();
        for &doc in CanonicalDoc::all() {
            let path = self.vault_path.join(doc.filename());
            if !path.exists() {
                std::fs::write(&path, doc.default_template())?;
                created.push(doc);
            }
        }
        Ok(created)
    }

    // ── Internal helpers ─────────────────────────────────────────────────────

    fn write_raw(&self, doc: CanonicalDoc, content: &VaultDoc) -> Result<(), MemorySyncError> {
        let path = self.vault_path.join(doc.filename());
        std::fs::write(&path, content.render())?;
        Ok(())
    }
}

/// Reject any path that contains a `..` component.
fn check_no_traversal(path: &Path) -> Result<(), MemorySyncError> {
    for component in path.components() {
        if component == std::path::Component::ParentDir {
            return Err(MemorySyncError::PathTraversal(path.display().to_string()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docs::CanonicalDoc;

    #[test]
    fn open_nonexistent_path_errors() {
        let result = VaultAccessor::open("/nonexistent/vault/path/xyz");
        assert!(result.is_err());
    }

    #[test]
    fn read_doc_missing_returns_none() {
        let dir = tempfile::TempDir::new().unwrap();
        let vault = VaultAccessor::open(dir.path()).unwrap();
        let result = vault.read_doc(CanonicalDoc::Soul).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn write_readonly_doc_returns_error() {
        let dir = tempfile::TempDir::new().unwrap();
        let vault = VaultAccessor::open(dir.path()).unwrap();
        let doc = VaultDoc::parse("# Soul\n\nContent.\n").unwrap();
        let result = vault.write_doc(CanonicalDoc::Soul, &doc);
        assert!(matches!(result, Err(MemorySyncError::ReadOnly(_))));
    }

    #[test]
    fn write_approval_required_doc_returns_error() {
        let dir = tempfile::TempDir::new().unwrap();
        let vault = VaultAccessor::open(dir.path()).unwrap();
        let doc = VaultDoc::parse("# User\n\nPrefs.\n").unwrap();
        let result = vault.write_doc(CanonicalDoc::User, &doc);
        assert!(matches!(result, Err(MemorySyncError::ApprovalRequired(_))));
    }

    #[test]
    fn write_runtime_writable_doc_succeeds() {
        let dir = tempfile::TempDir::new().unwrap();
        let vault = VaultAccessor::open(dir.path()).unwrap();
        let doc =
            VaultDoc::parse("# Heartbeat\n\n<!-- machine-managed -->\nStatus: ok\n").unwrap();
        vault.write_doc(CanonicalDoc::Heartbeat, &doc).unwrap();

        let read_back = vault.read_doc(CanonicalDoc::Heartbeat).unwrap().unwrap();
        let sec = read_back.section("Heartbeat").unwrap();
        assert!(sec.content.contains("Status: ok"));
    }

    #[test]
    fn update_machine_sections_only_updates_machine_managed() {
        let dir = tempfile::TempDir::new().unwrap();
        let vault = VaultAccessor::open(dir.path()).unwrap();

        let initial = "# Heartbeat\n\n<!-- machine-managed -->\nStatus: idle\n";
        std::fs::write(dir.path().join("HEARTBEAT.md"), initial).unwrap();

        vault
            .update_machine_sections(
                CanonicalDoc::Heartbeat,
                &[("Heartbeat", "Status: running")],
            )
            .unwrap();

        let doc = vault.read_doc(CanonicalDoc::Heartbeat).unwrap().unwrap();
        let sec = doc.section("Heartbeat").unwrap();
        assert!(sec.content.contains("Status: running"));
        assert!(!sec.content.contains("Status: idle"));
    }

    #[test]
    fn init_defaults_creates_missing_docs() {
        let dir = tempfile::TempDir::new().unwrap();
        let vault = VaultAccessor::open(dir.path()).unwrap();
        let created = vault.init_defaults().unwrap();
        assert_eq!(created.len(), CanonicalDoc::all().len());

        for &doc in CanonicalDoc::all() {
            assert!(dir.path().join(doc.filename()).exists());
        }
    }

    #[test]
    fn path_traversal_rejected() {
        let dir = tempfile::TempDir::new().unwrap();
        let vault = VaultAccessor::open(dir.path()).unwrap();
        let evil = Path::new("../evil.md");
        let result = vault.read_file(evil);
        assert!(matches!(result, Err(MemorySyncError::PathTraversal(_))));
    }
}
