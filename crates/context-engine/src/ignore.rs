//! Ignore rule engine.
//!
//! Wraps the `ignore` crate to apply `.gitignore` semantics plus
//! optional tool-specific ignore rules loaded from `.contextignore`.
//!
//! # `.contextignore` format
//! Same glob syntax as `.gitignore`. Place in the project root.
//! Lines starting with `#` are comments. Blank lines are ignored.
//!
//! # Example patterns
//! ```text
//! target/
//! *.lock
//! node_modules/
//! __pycache__/
//! ```

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::path::{Path, PathBuf};

/// Controls which files are ignored during scanning.
pub struct IgnoreEngine {
    root: PathBuf,
    gitignore: Option<Gitignore>,
    tool_ignore: Option<Gitignore>,
}

impl IgnoreEngine {
    /// Build an ignore engine for the given project root.
    ///
    /// Loads `.gitignore` and `.contextignore` from the root if present.
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        let gitignore = load_gitignore(&root, ".gitignore");
        let tool_ignore = load_gitignore(&root, ".contextignore");
        Self { root, gitignore, tool_ignore }
    }

    /// Returns `true` if the given path should be ignored.
    ///
    /// `path` may be absolute or relative to the project root.
    pub fn is_ignored(&self, path: &Path, is_dir: bool) -> bool {
        // Always ignore hidden files/dirs (except .env.example)
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.') && name != ".env.example" {
                return true;
            }
        }

        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root.join(path)
        };

        if let Some(gi) = &self.gitignore {
            if gi.matched(&path, is_dir).is_ignore() {
                return true;
            }
        }
        if let Some(ti) = &self.tool_ignore {
            if ti.matched(&path, is_dir).is_ignore() {
                return true;
            }
        }
        false
    }

    /// Returns true if the given file extension is a known binary/non-text type.
    pub fn is_binary_extension(ext: &str) -> bool {
        matches!(
            ext,
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "ico" | "svg" |
            "pdf" | "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" |
            "exe" | "dll" | "so" | "dylib" | "a" | "lib" |
            "wasm" | "bin" | "dat" |
            "mp3" | "mp4" | "avi" | "mkv" | "wav" | "flac" |
            "ttf" | "otf" | "woff" | "woff2" |
            "sqlite" | "db" | "sqlite3"
        )
    }
}

fn load_gitignore(root: &Path, filename: &str) -> Option<Gitignore> {
    let path = root.join(filename);
    if !path.exists() {
        return None;
    }
    let mut builder = GitignoreBuilder::new(root);
    let err = builder.add(&path);
    if err.is_some() {
        // parse error — skip this ignore file
        return None;
    }
    builder.build().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_engine_with_gitignore(dir: &TempDir, gitignore_content: &str) -> IgnoreEngine {
        fs::write(dir.path().join(".gitignore"), gitignore_content).unwrap();
        IgnoreEngine::new(dir.path())
    }

    #[test]
    fn ignores_target_directory() {
        let dir = TempDir::new().unwrap();
        let engine = make_engine_with_gitignore(&dir, "target/\n");
        let target = dir.path().join("target");
        assert!(engine.is_ignored(&target, true));
    }

    #[test]
    fn does_not_ignore_src() {
        let dir = TempDir::new().unwrap();
        let engine = make_engine_with_gitignore(&dir, "target/\n");
        let src = dir.path().join("src");
        assert!(!engine.is_ignored(&src, true));
    }

    #[test]
    fn ignores_lock_files() {
        let dir = TempDir::new().unwrap();
        let engine = make_engine_with_gitignore(&dir, "*.lock\n");
        let lock = dir.path().join("Cargo.lock");
        assert!(engine.is_ignored(&lock, false));
    }

    #[test]
    fn contextignore_overrides() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(".contextignore"), "docs/\n").unwrap();
        let engine = IgnoreEngine::new(dir.path());
        let docs = dir.path().join("docs");
        assert!(engine.is_ignored(&docs, true));
    }

    #[test]
    fn hidden_files_are_ignored() {
        let dir = TempDir::new().unwrap();
        let engine = IgnoreEngine::new(dir.path());
        let hidden = dir.path().join(".hidden_file");
        assert!(engine.is_ignored(&hidden, false));
    }

    #[test]
    fn binary_extensions_detected() {
        assert!(IgnoreEngine::is_binary_extension("png"));
        assert!(IgnoreEngine::is_binary_extension("exe"));
        assert!(!IgnoreEngine::is_binary_extension("rs"));
        assert!(!IgnoreEngine::is_binary_extension("toml"));
    }
}
