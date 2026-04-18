//! Filesystem scanner with ignore-rule support.
//!
//! Phase 0 stub.

use std::path::{Path, PathBuf};

/// A file discovered during scanning with its relevance score.
#[derive(Debug, Clone)]
pub struct ScoredFile {
    pub path: PathBuf,
    /// Relevance score in [0.0, 1.0]. Higher is more relevant.
    pub score: f32,
    /// Approximate token count for this file's content.
    pub token_estimate: u32,
}

/// Scans a directory for relevant files, respecting ignore rules.
///
/// Phase 0 stub — implementation deferred to Phase 6.
pub struct ContextScanner {
    root: PathBuf,
}

impl ContextScanner {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self { root: root.as_ref().to_path_buf() }
    }

    pub async fn scan(&self) -> Result<Vec<ScoredFile>, crate::error::ContextError> {
        // Phase 0 stub
        Ok(vec![])
    }
}
