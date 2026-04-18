//! Memory sync pipeline — bidirectional sync between the vault and the runtime.

use chrono::Utc;

use crate::docs::CanonicalDoc;
use crate::error::MemorySyncError;
use crate::markdown::VaultDoc;
use crate::vault::VaultAccessor;

/// Direction of a sync operation.
pub enum SyncDirection {
    /// Push structured memory records into vault docs.
    StoreToVault,
    /// Pull vault content into the retrieval index.
    VaultToIndex,
}

/// Summary of a completed sync operation.
pub struct SyncResult {
    pub direction: SyncDirection,
    pub docs_updated: Vec<CanonicalDoc>,
    pub notes_indexed: usize,
    pub conflicts: Vec<ConflictRecord>,
    pub errors: Vec<String>,
}

/// A detected conflict between expected and actual machine-managed content.
pub struct ConflictRecord {
    pub doc: String,
    pub section: Option<String>,
    pub reason: String,
    pub resolution: ConflictResolution,
}

pub enum ConflictResolution {
    /// Machine section overwritten (was already machine-managed).
    MachineOverwritten,
    /// Human section preserved (not machine-managed).
    HumanPreserved,
    /// Conflict flagged for manual review.
    RequiresReview,
}

const MACHINE_TAG: &str = "<!-- machine-managed -->";

/// Sync engine that drives vault ↔ runtime synchronisation.
pub struct SyncEngine {
    accessor: VaultAccessor,
}

impl SyncEngine {
    pub fn new(accessor: VaultAccessor) -> Self {
        Self { accessor }
    }

    /// Push runtime state into writable vault docs.
    ///
    /// Updates HEARTBEAT.md with `heartbeat_status` and current timestamp,
    /// and TOOLS.md with the provided `tool_names`.
    pub async fn sync_to_vault(
        &self,
        heartbeat_status: &str,
        tool_names: &[&str],
    ) -> Result<SyncResult, MemorySyncError> {
        let mut docs_updated = Vec::new();
        let mut errors = Vec::new();

        // ── HEARTBEAT.md ──────────────────────────────────────────────────
        let timestamp = Utc::now().to_rfc3339();
        let hb_body = format!(
            "Status: {}\nLast updated: {}",
            heartbeat_status, timestamp
        );
        match self
            .accessor
            .update_machine_sections(CanonicalDoc::Heartbeat, &[("Heartbeat", &hb_body)])
        {
            Ok(()) => docs_updated.push(CanonicalDoc::Heartbeat),
            Err(e) => errors.push(format!("HEARTBEAT.md: {}", e)),
        }

        // ── TOOLS.md ──────────────────────────────────────────────────────
        let tools_body = if tool_names.is_empty() {
            "No tools registered.".to_string()
        } else {
            tool_names.join("\n")
        };
        match self
            .accessor
            .update_machine_sections(CanonicalDoc::Tools, &[("Tools", &tools_body)])
        {
            Ok(()) => docs_updated.push(CanonicalDoc::Tools),
            Err(e) => errors.push(format!("TOOLS.md: {}", e)),
        }

        Ok(SyncResult {
            direction: SyncDirection::StoreToVault,
            docs_updated,
            notes_indexed: 0,
            conflicts: vec![],
            errors,
        })
    }

    /// Scan all vault docs and return their parsed contents for indexing.
    pub async fn index_vault(
        &self,
    ) -> Result<Vec<(String, VaultDoc)>, MemorySyncError> {
        let files = self.accessor.list_files()?;
        let mut result = Vec::new();

        for abs_path in files {
            let relative = match abs_path.strip_prefix(self.accessor.vault_path()) {
                Ok(r) => r.to_path_buf(),
                Err(_) => continue,
            };
            if let Some(content) = self.accessor.read_file(&relative)? {
                match VaultDoc::parse(&content) {
                    Ok(doc) => {
                        result.push((relative.to_string_lossy().to_string(), doc));
                    }
                    Err(e) => {
                        tracing::warn!("Failed to parse {:?}: {}", relative, e);
                    }
                }
            }
        }

        Ok(result)
    }

    /// Detect whether a machine-managed section in `doc` has been manually
    /// edited compared to `expected_content`.
    ///
    /// `expected_content` is the body that was last written (without the
    /// `<!-- machine-managed -->` tag).  Returns `None` if everything matches
    /// or there is no machine-managed section to compare.
    pub fn detect_conflicts(
        &self,
        doc: CanonicalDoc,
        expected_content: &str,
    ) -> Result<Option<ConflictRecord>, MemorySyncError> {
        let vault_doc = match self.accessor.read_doc(doc)? {
            None => return Ok(None),
            Some(d) => d,
        };

        for section in &vault_doc.sections {
            if !section.machine_managed {
                continue;
            }

            // Extract body after the machine-managed tag.
            let actual_body = if let Some(pos) = section.content.find(MACHINE_TAG) {
                section.content[pos + MACHINE_TAG.len()..]
                    .trim_start_matches('\n')
            } else {
                section.content.trim()
            };

            if actual_body.trim() != expected_content.trim() {
                return Ok(Some(ConflictRecord {
                    doc: doc.filename().to_string(),
                    section: section.heading.clone(),
                    reason: "machine-managed section content differs from expected".to_string(),
                    resolution: ConflictResolution::RequiresReview,
                }));
            }
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_engine(dir: &tempfile::TempDir) -> SyncEngine {
        let vault = VaultAccessor::open(dir.path()).unwrap();
        SyncEngine::new(vault)
    }

    #[tokio::test]
    async fn sync_to_vault_updates_heartbeat() {
        let dir = tempfile::TempDir::new().unwrap();
        let engine = make_engine(&dir);

        let result = engine.sync_to_vault("running", &[]).await.unwrap();
        assert!(result.errors.is_empty());
        assert!(result.docs_updated.contains(&CanonicalDoc::Heartbeat));

        let vault = VaultAccessor::open(dir.path()).unwrap();
        let doc = vault.read_doc(CanonicalDoc::Heartbeat).unwrap().unwrap();
        let sec = doc.section("Heartbeat").unwrap();
        assert!(sec.content.contains("Status: running"));
        assert!(sec.content.contains("Last updated:"));
    }

    #[tokio::test]
    async fn sync_to_vault_updates_tools() {
        let dir = tempfile::TempDir::new().unwrap();
        let engine = make_engine(&dir);

        let result = engine
            .sync_to_vault("idle", &["bash", "read_file"])
            .await
            .unwrap();
        assert!(result.errors.is_empty());
        assert!(result.docs_updated.contains(&CanonicalDoc::Tools));

        let vault = VaultAccessor::open(dir.path()).unwrap();
        let doc = vault.read_doc(CanonicalDoc::Tools).unwrap().unwrap();
        let sec = doc.section("Tools").unwrap();
        assert!(sec.content.contains("bash"));
        assert!(sec.content.contains("read_file"));
    }

    #[tokio::test]
    async fn index_vault_returns_all_docs() {
        let dir = tempfile::TempDir::new().unwrap();
        {
            let setup = VaultAccessor::open(dir.path()).unwrap();
            setup.init_defaults().unwrap();
        }
        let engine = make_engine(&dir);
        let indexed = engine.index_vault().await.unwrap();
        assert_eq!(indexed.len(), CanonicalDoc::all().len());
    }

    #[test]
    fn detect_conflicts_none_when_matches() {
        let dir = tempfile::TempDir::new().unwrap();
        let expected = "Status: idle\nLast updated: 2024-01-01";
        {
            let setup = VaultAccessor::open(dir.path()).unwrap();
            setup
                .update_machine_sections(
                    CanonicalDoc::Heartbeat,
                    &[("Heartbeat", expected)],
                )
                .unwrap();
        }
        let engine = make_engine(&dir);
        let conflict = engine
            .detect_conflicts(CanonicalDoc::Heartbeat, expected)
            .unwrap();
        assert!(conflict.is_none());
    }

    #[test]
    fn detect_conflicts_some_when_manually_edited() {
        let dir = tempfile::TempDir::new().unwrap();
        let expected = "Status: idle\nLast updated: 2024-01-01";
        {
            let setup = VaultAccessor::open(dir.path()).unwrap();
            setup
                .update_machine_sections(
                    CanonicalDoc::Heartbeat,
                    &[("Heartbeat", expected)],
                )
                .unwrap();
            // Simulate manual edit.
            setup
                .update_machine_sections(
                    CanonicalDoc::Heartbeat,
                    &[("Heartbeat", "Status: MANUALLY CHANGED\nLast updated: 2024-01-01")],
                )
                .unwrap();
        }
        let engine = make_engine(&dir);
        let conflict = engine
            .detect_conflicts(CanonicalDoc::Heartbeat, expected)
            .unwrap();
        assert!(conflict.is_some());
        let rec = conflict.unwrap();
        assert_eq!(rec.doc, "HEARTBEAT.md");
    }
}
