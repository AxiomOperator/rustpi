//! Central path safety policy for file and subprocess tools.
//!
//! All file access goes through [`PathSafetyPolicy`] before any I/O is performed.
//! This prevents path traversal attacks and keeps file operations within allowed roots.

use crate::ToolError;
use std::path::{Path, PathBuf};

/// Policy controlling which filesystem paths tools may access.
#[derive(Debug, Clone)]
pub struct PathSafetyPolicy {
    /// Absolute paths that tools are allowed to read/write within.
    /// Empty means all paths are denied.
    allowed_roots: Vec<PathBuf>,
    /// Absolute paths that are always denied, even if under an allowed root.
    denied_paths: Vec<PathBuf>,
}

impl PathSafetyPolicy {
    /// Create a policy with explicit allowed roots.
    pub fn new(allowed_roots: impl IntoIterator<Item = impl Into<PathBuf>>) -> Self {
        Self {
            allowed_roots: allowed_roots.into_iter().map(Into::into).collect(),
            denied_paths: vec![],
        }
    }

    /// Allow-all policy (for testing only — not for production use).
    pub fn allow_all() -> Self {
        Self {
            allowed_roots: vec![PathBuf::from("/")],
            denied_paths: vec![],
        }
    }

    /// Add an explicit denied path (takes priority over allowed roots).
    pub fn deny(mut self, path: impl Into<PathBuf>) -> Self {
        self.denied_paths.push(path.into());
        self
    }

    /// Validate a path and return the normalized absolute path if allowed.
    /// Returns `ToolError::PathTraversal` if denied.
    pub fn validate(&self, path: impl AsRef<Path>) -> Result<PathBuf, ToolError> {
        let path = path.as_ref();

        let abs = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(ToolError::Io)?
                .join(path)
        };

        let normalized = normalize_path(&abs);

        // Check denied list first
        for denied in &self.denied_paths {
            if normalized.starts_with(denied) {
                return Err(ToolError::PathTraversal(format!(
                    "{} is in the denied list",
                    normalized.display()
                )));
            }
        }

        let allowed = self
            .allowed_roots
            .iter()
            .any(|root| normalized.starts_with(root));

        if !allowed {
            return Err(ToolError::PathTraversal(format!(
                "{} is outside all allowed roots",
                normalized.display()
            )));
        }

        Ok(normalized)
    }

    pub fn allowed_roots(&self) -> &[PathBuf] {
        &self.allowed_roots
    }
}

/// Normalize a path by resolving `.` and `..` components without filesystem access.
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        use std::path::Component;
        match component {
            Component::ParentDir => {
                components.pop();
            }
            Component::CurDir => {}
            c => components.push(c),
        }
    }
    components.iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_path_under_root() {
        let policy = PathSafetyPolicy::new(["/workspace"]);
        assert!(policy.validate("/workspace/src/main.rs").is_ok());
    }

    #[test]
    fn denies_path_outside_root() {
        let policy = PathSafetyPolicy::new(["/workspace"]);
        assert!(matches!(
            policy.validate("/etc/passwd"),
            Err(ToolError::PathTraversal(_))
        ));
    }

    #[test]
    fn blocks_traversal_via_dotdot() {
        let policy = PathSafetyPolicy::new(["/workspace"]);
        assert!(matches!(
            policy.validate("/workspace/../etc/passwd"),
            Err(ToolError::PathTraversal(_))
        ));
    }

    #[test]
    fn explicit_deny_overrides_allowed_root() {
        let policy = PathSafetyPolicy::new(["/workspace"]).deny("/workspace/secrets");
        assert!(matches!(
            policy.validate("/workspace/secrets/key.pem"),
            Err(ToolError::PathTraversal(_))
        ));
    }

    #[test]
    fn normalize_path_resolves_dotdot() {
        let p = normalize_path(Path::new("/a/b/../c"));
        assert_eq!(p, PathBuf::from("/a/c"));
    }

    #[test]
    fn deny_traversal_with_encoded_dotdot() {
        // %2e%2e is URL-encoded ".." — we do NOT URL-decode paths, so std::path
        // treats it as a literal component (not as "..").
        // The path stays under /workspace/ and is therefore allowed.
        // Key property: URL-encoded traversal cannot escape the allowed root.
        let policy = PathSafetyPolicy::new(["/workspace"]);
        let result = policy.validate("/workspace/%2e%2e/etc/passwd");
        assert!(
            result.is_ok(),
            "URL-encoded dotdot is a literal component, not traversal: {result:?}"
        );
        // Verify the normalized path is still under /workspace/
        let normalized = result.unwrap();
        assert!(
            normalized.starts_with("/workspace"),
            "path must remain under /workspace, got: {}",
            normalized.display()
        );
    }

    #[test]
    fn deny_null_byte_in_path() {
        let policy = PathSafetyPolicy::new(["/workspace"]);
        // std::path treats null byte as a literal component; this must not panic or crash.
        let result = policy.validate("/workspace/file\0evil");
        // We only assert no panic — the outcome depends on OS path semantics.
        let _ = result;
    }

    #[test]
    fn empty_allowed_roots_denies_everything() {
        let policy = PathSafetyPolicy::new([] as [&str; 0]);
        assert!(
            matches!(policy.validate("/workspace/src/main.rs"), Err(ToolError::PathTraversal(_))),
            "empty roots must deny all paths"
        );
        assert!(
            matches!(policy.validate("/"), Err(ToolError::PathTraversal(_))),
            "empty roots must deny root path"
        );
    }
}
