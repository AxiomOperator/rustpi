//! Filesystem scanner.
//!
//! Recursively discovers files in a project directory, applying ignore rules
//! before collecting metadata. Does not load file contents eagerly.

use crate::{error::ContextError, ignore::IgnoreEngine};
use std::path::{Path, PathBuf};

/// Metadata for a discovered file. Contents are loaded lazily.
#[derive(Debug, Clone)]
pub struct FileEntry {
    /// Absolute path to the file.
    pub path: PathBuf,
    /// File size in bytes.
    pub size_bytes: u64,
    /// File extension (lowercase), if any.
    pub extension: Option<String>,
    /// Whether this file is likely a source/text file (not binary).
    pub is_text: bool,
}

impl FileEntry {
    /// Load the file contents as UTF-8 text.
    /// Returns `None` if the file cannot be read as valid UTF-8.
    pub async fn read_text(&self) -> Option<String> {
        tokio::fs::read_to_string(&self.path).await.ok()
    }
}

/// Configuration for the filesystem scanner.
#[derive(Debug, Clone)]
pub struct ScannerConfig {
    /// Maximum file size in bytes to include (default: 512KB).
    pub max_file_size_bytes: u64,
    /// Maximum number of files to return (default: 1000).
    pub max_files: usize,
    /// Whether to follow symlinks (default: false).
    pub follow_symlinks: bool,
}

impl Default for ScannerConfig {
    fn default() -> Self {
        Self {
            max_file_size_bytes: 512 * 1024,
            max_files: 1000,
            follow_symlinks: false,
        }
    }
}

/// Scanner statistics for observability.
#[derive(Debug, Default, Clone)]
pub struct ScanStats {
    pub dirs_visited: usize,
    pub files_found: usize,
    pub files_ignored: usize,
    pub files_too_large: usize,
    pub files_binary: usize,
}

/// Scans a project directory for candidate files.
pub struct Scanner {
    root: PathBuf,
    ignore: IgnoreEngine,
    config: ScannerConfig,
}

impl Scanner {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        let ignore = IgnoreEngine::new(&root);
        Self { root, ignore, config: ScannerConfig::default() }
    }

    pub fn with_config(mut self, config: ScannerConfig) -> Self {
        self.config = config;
        self
    }

    /// Scan the project root and return discovered files.
    pub async fn scan(&self) -> Result<(Vec<FileEntry>, ScanStats), ContextError> {
        let mut entries = Vec::new();
        let mut stats = ScanStats::default();
        self.scan_dir(&self.root.clone(), &mut entries, &mut stats)?;
        Ok((entries, stats))
    }

    fn scan_dir(
        &self,
        dir: &Path,
        entries: &mut Vec<FileEntry>,
        stats: &mut ScanStats,
    ) -> Result<(), ContextError> {
        if entries.len() >= self.config.max_files {
            return Ok(());
        }

        let read_dir = match std::fs::read_dir(dir) {
            Ok(rd) => rd,
            Err(_) => return Ok(()), // skip unreadable dirs
        };

        stats.dirs_visited += 1;

        let mut children: Vec<_> = read_dir.flatten().collect();
        // Sort for deterministic output
        children.sort_by_key(|e| e.path());

        for entry in children {
            if entries.len() >= self.config.max_files {
                break;
            }

            let path = entry.path();
            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };

            if metadata.is_symlink() && !self.config.follow_symlinks {
                continue;
            }

            if metadata.is_dir() {
                if self.ignore.is_ignored(&path, true) {
                    stats.files_ignored += 1;
                    continue;
                }
                self.scan_dir(&path, entries, stats)?;
            } else if metadata.is_file() {
                if self.ignore.is_ignored(&path, false) {
                    stats.files_ignored += 1;
                    continue;
                }

                let size = metadata.len();
                if size > self.config.max_file_size_bytes {
                    stats.files_too_large += 1;
                    continue;
                }

                let extension = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.to_lowercase());

                let is_binary = extension
                    .as_deref()
                    .map(IgnoreEngine::is_binary_extension)
                    .unwrap_or(false);

                if is_binary {
                    stats.files_binary += 1;
                    continue;
                }

                stats.files_found += 1;
                entries.push(FileEntry {
                    path,
                    size_bytes: size,
                    extension,
                    is_text: true,
                });
            }
        }

        Ok(())
    }
}
