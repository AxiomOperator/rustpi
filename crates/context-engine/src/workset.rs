//! Working-set selection.
//!
//! Selects the most relevant files from scored candidates, bounded by
//! file count and estimated token budget, while preserving diversity
//! (avoiding over-representation of a single directory).

use crate::{relevance::ScoredEntry, tokens};
use std::path::PathBuf;

/// Configuration for working-set selection.
#[derive(Debug, Clone)]
pub struct WorksetConfig {
    /// Maximum number of files in the working set.
    pub max_files: usize,
    /// Token budget available for file content (0 = unlimited).
    pub token_budget: u32,
    /// Minimum relevance score to include (0.0 = include all).
    pub min_score: f32,
    /// Maximum files from any single directory (for diversity).
    pub max_per_dir: usize,
}

impl Default for WorksetConfig {
    fn default() -> Self {
        Self {
            max_files: 50,
            token_budget: 32_000,
            min_score: 0.0,
            max_per_dir: 10,
        }
    }
}

/// A selected working set of files with token estimates.
#[derive(Debug, Clone)]
pub struct WorkingSet {
    pub selected: Vec<SelectedFile>,
    pub excluded: Vec<ScoredEntry>,
    pub estimated_tokens: u32,
}

/// A file selected for inclusion in the working set.
#[derive(Debug, Clone)]
pub struct SelectedFile {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub score: f32,
    pub estimated_tokens: u32,
    /// Reason the file was selected (for observability).
    pub reason: SelectionReason,
}

/// Why a file was selected.
#[derive(Debug, Clone, PartialEq)]
pub enum SelectionReason {
    TopRelevance,
    ExplicitlyReferenced,
    BudgetTruncated,
}

/// Select a bounded working set from scored entries.
pub fn select(entries: Vec<ScoredEntry>, config: &WorksetConfig) -> WorkingSet {
    let mut selected = Vec::new();
    let mut excluded = Vec::new();
    let mut total_tokens = 0u32;
    let mut dir_counts: std::collections::HashMap<PathBuf, usize> =
        std::collections::HashMap::new();

    for scored in entries {
        if scored.score < config.min_score {
            excluded.push(scored);
            continue;
        }

        if selected.len() >= config.max_files {
            excluded.push(scored);
            continue;
        }

        // Diversity check: limit files from any one directory
        let parent = scored
            .entry
            .path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default();
        let dir_count = dir_counts.get(&parent).copied().unwrap_or(0);
        if dir_count >= config.max_per_dir {
            excluded.push(scored);
            continue;
        }

        let file_tokens = tokens::estimate_bytes(scored.entry.size_bytes);

        if config.token_budget > 0 {
            if total_tokens + file_tokens > config.token_budget {
                excluded.push(scored);
                continue;
            }
        }

        total_tokens += file_tokens;
        *dir_counts.entry(parent).or_insert(0) += 1;

        selected.push(SelectedFile {
            path: scored.entry.path.clone(),
            size_bytes: scored.entry.size_bytes,
            score: scored.score,
            estimated_tokens: file_tokens,
            reason: SelectionReason::TopRelevance,
        });
    }

    WorkingSet { selected, excluded, estimated_tokens: total_tokens }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relevance::ScoredEntry;
    use crate::scanner::FileEntry;

    fn scored(path: &str, score: f32, size: u64) -> ScoredEntry {
        ScoredEntry {
            entry: FileEntry {
                path: path.into(),
                size_bytes: size,
                extension: None,
                is_text: true,
            },
            score,
        }
    }

    #[test]
    fn select_respects_max_files() {
        let entries: Vec<_> =
            (0..20).map(|i| scored(&format!("/p/file{i}.rs"), 0.5, 100)).collect();
        let config = WorksetConfig { max_files: 5, ..Default::default() };
        let ws = select(entries, &config);
        assert_eq!(ws.selected.len(), 5);
        assert_eq!(ws.excluded.len(), 15);
    }

    #[test]
    fn select_respects_token_budget() {
        // 10 files × 400 bytes each = 100 tokens each → budget of 300 fits 3
        let entries: Vec<_> =
            (0..10).map(|i| scored(&format!("/p/f{i}.rs"), 0.5, 400)).collect();
        let config = WorksetConfig {
            token_budget: 300,
            max_files: 100,
            ..Default::default()
        };
        let ws = select(entries, &config);
        assert!(ws.estimated_tokens <= 300);
        assert!(ws.selected.len() <= 3);
    }

    #[test]
    fn select_filters_by_min_score() {
        let entries = vec![
            scored("/p/high.rs", 0.8, 100),
            scored("/p/low.rs", 0.1, 100),
        ];
        let config = WorksetConfig { min_score: 0.5, ..Default::default() };
        let ws = select(entries, &config);
        assert_eq!(ws.selected.len(), 1);
        assert_eq!(ws.selected[0].path, std::path::PathBuf::from("/p/high.rs"));
    }

    #[test]
    fn select_diversity_limits_per_dir() {
        // 5 files all in same dir, max_per_dir = 2
        let entries: Vec<_> = (0..5)
            .map(|i| scored(&format!("/p/dir/f{i}.rs"), 0.5, 100))
            .collect();
        let config = WorksetConfig {
            max_per_dir: 2,
            max_files: 100,
            token_budget: 0, // unlimited
            ..Default::default()
        };
        let ws = select(entries, &config);
        assert_eq!(ws.selected.len(), 2);
    }
}
