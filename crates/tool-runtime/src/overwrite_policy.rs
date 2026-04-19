//! File overwrite safety policy.
//!
//! Controls what happens when a write or edit targets an existing file.
//! Applied BEFORE any I/O is performed.

use crate::ToolError;
use std::path::Path;

/// Policy applied when a tool attempts to overwrite an existing file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum OverwritePolicy {
    /// Allow overwrites silently (default — matches existing behavior).
    #[default]
    Allow,
    /// Deny overwrites of existing files. New file creation is still allowed.
    DenyExisting,
    /// Require explicit `overwrite: true` flag in the tool call arguments.
    /// If not present and file exists, returns ToolError::OverwriteNotConfirmed.
    RequireConfirmation,
}

impl OverwritePolicy {
    /// Check whether a write to `path` is permitted.
    /// `overwrite_confirmed` is true if the caller explicitly passed `overwrite: true`.
    pub fn check(&self, path: &Path, overwrite_confirmed: bool) -> Result<(), ToolError> {
        let exists = path.exists();
        match self {
            OverwritePolicy::Allow => Ok(()),
            OverwritePolicy::DenyExisting => {
                if exists {
                    Err(ToolError::OverwriteDenied(
                        path.to_string_lossy().into_owned(),
                    ))
                } else {
                    Ok(())
                }
            }
            OverwritePolicy::RequireConfirmation => {
                if exists && !overwrite_confirmed {
                    Err(ToolError::OverwriteNotConfirmed(
                        path.to_string_lossy().into_owned(),
                    ))
                } else {
                    Ok(())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn allow_policy_new_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("new.txt");
        assert!(OverwritePolicy::Allow.check(&path, false).is_ok());
    }

    #[test]
    fn allow_policy_existing_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("existing.txt");
        std::fs::write(&path, "data").unwrap();
        assert!(OverwritePolicy::Allow.check(&path, false).is_ok());
    }

    #[test]
    fn deny_existing_new_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("new.txt");
        assert!(OverwritePolicy::DenyExisting.check(&path, false).is_ok());
    }

    #[test]
    fn deny_existing_existing_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("existing.txt");
        std::fs::write(&path, "data").unwrap();
        assert!(matches!(
            OverwritePolicy::DenyExisting.check(&path, false),
            Err(ToolError::OverwriteDenied(_))
        ));
    }

    #[test]
    fn require_confirmation_existing_confirmed() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("existing.txt");
        std::fs::write(&path, "data").unwrap();
        assert!(OverwritePolicy::RequireConfirmation
            .check(&path, true)
            .is_ok());
    }

    #[test]
    fn require_confirmation_existing_not_confirmed() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("existing.txt");
        std::fs::write(&path, "data").unwrap();
        assert!(matches!(
            OverwritePolicy::RequireConfirmation.check(&path, false),
            Err(ToolError::OverwriteNotConfirmed(_))
        ));
    }

    #[test]
    fn require_confirmation_new_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("new.txt");
        assert!(OverwritePolicy::RequireConfirmation
            .check(&path, false)
            .is_ok());
    }
}
