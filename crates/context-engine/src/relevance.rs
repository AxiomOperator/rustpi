//! File relevance scoring.
//!
//! Scores candidate files on a 0.0–1.0 scale using heuristics tied to
//! project conventions and context signals. Higher = more relevant.
//!
//! # Scoring heuristics (additive, capped at 1.0)
//! - Source files (`.rs`, `.py`, `.ts`, `.go`, etc.): +0.4
//! - Project config files (`Cargo.toml`, `package.json`, `pyproject.toml`, etc.): +0.35
//! - Documentation/README at root: +0.25
//! - Test files: +0.2
//! - Files in root directory (not buried deep): +0.1
//! - Smaller files (easier to fit in budget): +0.05
//! - Files with names matching hint keywords: +0.2

use crate::scanner::FileEntry;
use std::path::Path;

/// Hints that influence relevance scoring for the current task.
#[derive(Debug, Default, Clone)]
pub struct RelevanceHints {
    /// Keywords from the user's task/query. Files whose paths contain these score higher.
    pub keywords: Vec<String>,
    /// Paths explicitly referenced in the current session/task.
    pub referenced_paths: Vec<std::path::PathBuf>,
    /// Project root (used to compute depth).
    pub root: Option<std::path::PathBuf>,
}

/// Score a file entry for relevance.
///
/// Returns a score in [0.0, 1.0].
pub fn score(entry: &FileEntry, hints: &RelevanceHints) -> f32 {
    let mut score = 0.0_f32;
    let path = &entry.path;
    let ext = entry.extension.as_deref().unwrap_or("");
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let name_lower = name.to_lowercase();

    // Source file bonus
    if is_source_extension(ext) {
        score += 0.4;
    }

    // Config file bonus
    if is_config_file(&name_lower) {
        score += 0.35;
    }

    // Documentation bonus
    if is_doc_file(&name_lower) {
        score += 0.25;
    }

    // Test file bonus
    if is_test_file(&name_lower, path) {
        score += 0.2;
    }

    // Depth bonus: files closer to root are more likely to be central
    if let Some(root) = &hints.root {
        if let Ok(rel) = path.strip_prefix(root) {
            let depth = rel.components().count();
            if depth == 1 {
                score += 0.15;
            } else if depth == 2 {
                score += 0.1;
            }
        }
    }

    // Size bonus: smaller files are easier to fit
    if entry.size_bytes < 2_000 {
        score += 0.05;
    }

    // Keyword match bonus
    let path_str = path.to_string_lossy().to_lowercase();
    for keyword in &hints.keywords {
        if path_str.contains(&keyword.to_lowercase()) {
            score += 0.2;
            break; // only count once
        }
    }

    // Explicitly referenced path bonus
    if hints.referenced_paths.iter().any(|p| p == path) {
        score += 0.3;
    }

    score.min(1.0)
}

/// Score all entries and return them sorted by score descending.
pub fn score_all(entries: Vec<FileEntry>, hints: &RelevanceHints) -> Vec<ScoredEntry> {
    let mut scored: Vec<ScoredEntry> = entries
        .into_iter()
        .map(|e| {
            let s = score(&e, hints);
            ScoredEntry { entry: e, score: s }
        })
        .collect();
    // Sort by score descending, then path ascending for stability
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.entry.path.cmp(&b.entry.path))
    });
    scored
}

/// A file entry paired with its relevance score.
#[derive(Debug, Clone)]
pub struct ScoredEntry {
    pub entry: FileEntry,
    /// Relevance score in [0.0, 1.0].
    pub score: f32,
}

fn is_source_extension(ext: &str) -> bool {
    matches!(
        ext,
        "rs" | "py" | "ts" | "tsx" | "js" | "jsx" | "go" | "java" | "kt" |
        "swift" | "c" | "cpp" | "h" | "hpp" | "cs" | "rb" | "php" | "scala" |
        "hs" | "ml" | "ex" | "exs" | "clj" | "lua" | "sh" | "bash" | "zsh"
    )
}

fn is_config_file(name: &str) -> bool {
    matches!(
        name,
        "cargo.toml" | "cargo.lock" | "package.json" | "package-lock.json" |
        "pyproject.toml" | "setup.py" | "setup.cfg" | "requirements.txt" |
        "go.mod" | "go.sum" | "pom.xml" | "build.gradle" | "build.gradle.kts" |
        "gemfile" | "gemfile.lock" | "composer.json" | "mix.exs" |
        "tsconfig.json" | "webpack.config.js" | "vite.config.ts" |
        "dockerfile" | "docker-compose.yml" | "docker-compose.yaml" |
        ".env.example" | "justfile" | "makefile" | "rakefile"
    )
}

fn is_doc_file(name: &str) -> bool {
    matches!(
        name,
        "readme.md" | "readme.txt" | "readme" |
        "contributing.md" | "changelog.md" | "license" | "license.md" |
        "architecture.md" | "design.md" | "project.md"
    )
}

fn is_test_file(name: &str, path: &Path) -> bool {
    name.starts_with("test_") ||
    name.ends_with("_test.rs") ||
    name.ends_with("_test.py") ||
    name.ends_with(".test.ts") ||
    name.ends_with(".spec.ts") ||
    name.ends_with(".test.js") ||
    path.components().any(|c| {
        c.as_os_str().to_str()
            .map(|s| s == "tests" || s == "test" || s == "__tests__")
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn entry(path: &str, size: u64, ext: Option<&str>) -> FileEntry {
        crate::scanner::FileEntry {
            path: PathBuf::from(path),
            size_bytes: size,
            extension: ext.map(|s| s.to_string()),
            is_text: true,
        }
    }

    fn hints_with_root(root: &str) -> RelevanceHints {
        RelevanceHints {
            root: Some(PathBuf::from(root)),
            ..Default::default()
        }
    }

    #[test]
    fn rust_source_scores_higher_than_binary() {
        let src = entry("/proj/src/main.rs", 1000, Some("rs"));
        let bin = entry("/proj/target/debug/proj", 500_000, None);
        let hints = hints_with_root("/proj");
        assert!(score(&src, &hints) > score(&bin, &hints));
    }

    #[test]
    fn cargo_toml_scores_high() {
        let cargo = entry("/proj/Cargo.toml", 500, Some("toml"));
        let hints = hints_with_root("/proj");
        assert!(score(&cargo, &hints) > 0.3);
    }

    #[test]
    fn keyword_match_boosts_score() {
        let matched = entry("/proj/src/auth.rs", 1000, Some("rs"));
        let unmatched = entry("/proj/src/main.rs", 1000, Some("rs"));
        let hints = RelevanceHints {
            keywords: vec!["auth".to_string()],
            root: Some(PathBuf::from("/proj")),
            referenced_paths: vec![],
        };
        assert!(score(&matched, &hints) > score(&unmatched, &hints));
    }

    #[test]
    fn referenced_path_boosts_score() {
        let refpath = PathBuf::from("/proj/src/main.rs");
        let entry1 = entry("/proj/src/main.rs", 1000, Some("rs"));
        let entry2 = entry("/proj/src/lib.rs", 1000, Some("rs"));
        let hints = RelevanceHints {
            referenced_paths: vec![refpath],
            root: Some(PathBuf::from("/proj")),
            ..Default::default()
        };
        assert!(score(&entry1, &hints) > score(&entry2, &hints));
    }

    #[test]
    fn score_all_sorts_descending() {
        let entries = vec![
            entry("/proj/misc.txt", 5000, Some("txt")),
            entry("/proj/Cargo.toml", 500, Some("toml")),
            entry("/proj/src/main.rs", 1000, Some("rs")),
        ];
        let hints = hints_with_root("/proj");
        let scored = score_all(entries, &hints);
        for i in 1..scored.len() {
            assert!(scored[i - 1].score >= scored[i].score);
        }
    }
}
