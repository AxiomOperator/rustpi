//! Top-level context engine orchestration.
//!
//! The [`ContextEngine`] ties together scanning, ignore rules, relevance scoring,
//! working-set selection, memory retrieval, context packing, and compaction
//! into a single reusable pipeline.
//!
//! # Pipeline
//! ```text
//! scan() → ignore filter → relevance score → workset select
//!   ↓
//! memory retrieve (parallel)
//!   ↓
//! pack (token budget)
//!   ↓
//! [compaction if over budget]
//!   ↓
//! PackedContext
//! ```

use crate::{
    compactor::{compact, CompactionStrategy},
    error::ContextError,
    memory::{MemoryQuery, MemoryRetriever, NoopMemory},
    packer::{ContextPacker, PackerConfig, PackedContext},
    relevance::{score_all, RelevanceHints},
    scanner::{Scanner, ScannerConfig, ScanStats},
    workset::{select, WorksetConfig},
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Configuration for the context engine.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Root directory to scan.
    pub project_root: PathBuf,
    /// Token budget for context packing.
    pub token_budget: u32,
    /// Token budget reserved for memory snippets.
    pub memory_budget: u32,
    /// Maximum files to scan.
    pub max_scan_files: usize,
    /// Maximum files in the working set.
    pub max_workset_files: usize,
    /// Minimum relevance score to include a file.
    pub min_relevance_score: f32,
    /// Maximum files per directory (diversity).
    pub max_per_dir: usize,
    /// Compaction threshold: if estimated tokens > budget * this factor, compact first.
    pub compaction_threshold: f32,
}

impl EngineConfig {
    pub fn new(project_root: impl AsRef<Path>) -> Self {
        Self {
            project_root: project_root.as_ref().to_path_buf(),
            token_budget: 32_000,
            memory_budget: 2_000,
            max_scan_files: 1000,
            max_workset_files: 50,
            min_relevance_score: 0.0,
            max_per_dir: 10,
            compaction_threshold: 1.5,
        }
    }
}

/// Statistics about a context engine run for observability.
#[derive(Debug, Default, Clone)]
pub struct EngineStats {
    pub scan: ScanStats,
    pub files_scored: usize,
    pub files_selected: usize,
    pub files_excluded: usize,
    pub memory_snippets: usize,
    pub total_tokens: u32,
    pub compacted: bool,
    pub truncated: bool,
}

/// The main context engine.
pub struct ContextEngine {
    config: EngineConfig,
    memory: Arc<dyn MemoryRetriever>,
}

impl ContextEngine {
    pub fn new(config: EngineConfig) -> Self {
        Self {
            config,
            memory: Arc::new(NoopMemory),
        }
    }

    pub fn with_memory(mut self, memory: Arc<dyn MemoryRetriever>) -> Self {
        self.memory = memory;
        self
    }

    /// Build a packed context for the given task/query.
    pub async fn build_context(
        &self,
        hints: RelevanceHints,
        memory_query: Option<MemoryQuery>,
    ) -> Result<(PackedContext, EngineStats), ContextError> {
        let mut stats = EngineStats::default();

        // 1. Scan filesystem
        let scanner = Scanner::new(&self.config.project_root).with_config(ScannerConfig {
            max_files: self.config.max_scan_files,
            ..Default::default()
        });
        let (entries, scan_stats) = scanner.scan().await?;
        stats.scan = scan_stats;

        if entries.is_empty() {
            return Err(ContextError::NoRelevantFiles(
                self.config.project_root.display().to_string(),
            ));
        }

        // 2. Score and rank
        let hints_with_root = RelevanceHints {
            root: Some(self.config.project_root.clone()),
            ..hints.clone()
        };
        let scored = score_all(entries, &hints_with_root);
        stats.files_scored = scored.len();

        // 3. Select working set
        let workset_config = WorksetConfig {
            max_files: self.config.max_workset_files,
            token_budget: (self.config.token_budget as f32 * self.config.compaction_threshold)
                as u32,
            min_score: self.config.min_relevance_score,
            max_per_dir: self.config.max_per_dir,
        };
        let mut workset = select(scored, &workset_config);
        stats.files_selected = workset.selected.len();
        stats.files_excluded = workset.excluded.len();

        // 4. Compact if needed
        let mut compact_summary: Option<String> = None;
        let estimated: u32 = workset.selected.iter().map(|f| f.estimated_tokens).sum();
        if estimated > self.config.token_budget {
            let (compacted_files, summary) = compact(
                workset.selected,
                self.config.token_budget,
                CompactionStrategy::DropLow { threshold: 0.3 },
            );
            workset.selected = compacted_files;
            compact_summary = Some(summary);
            stats.compacted = true;
        }

        // 5. Retrieve memory
        let mq = memory_query.unwrap_or_else(|| MemoryQuery {
            keywords: hints.keywords,
            total_token_budget: self.config.memory_budget,
            ..Default::default()
        });
        let memory_snippets = self.memory.retrieve(&mq).await;
        stats.memory_snippets = memory_snippets.len();

        // 6. Pack context
        let packer = ContextPacker::new(PackerConfig {
            token_budget: self.config.token_budget,
            memory_budget: self.config.memory_budget,
            ..Default::default()
        });
        let mut packed = packer.pack(workset.selected, memory_snippets).await?;
        packed.compact_summary = compact_summary;
        if packed.compact_summary.is_some() {
            packed.compacted = true;
        }

        stats.total_tokens = packed.total_tokens;
        stats.truncated = packed.truncated;

        Ok((packed, stats))
    }
}
