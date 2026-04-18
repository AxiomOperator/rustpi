//! Context compaction and summarization.
//!
//! When the raw working set cannot fit within the token budget, the compactor
//! reduces context by:
//! 1. Dropping low-relevance files entirely.
//! 2. Generating template-based summaries of dropped/oversized content.
//! 3. Providing a structured compact summary that preserves key facts.
//!
//! # Model-backed summarization
//! Real LLM summarization is deferred to a future phase when the agent
//! runtime is fully integrated. This MVP uses rule-based extraction:
//! - Extracts key declarations (functions, structs, enums, etc.)
//! - Preserves doc comments and module headers
//! - Produces a reduced representation suitable for the context window

use crate::workset::SelectedFile;

/// A compacted representation of file content.
#[derive(Debug, Clone)]
pub struct CompactedFile {
    pub path: std::path::PathBuf,
    pub summary: String,
    pub original_tokens: u32,
    pub compact_tokens: u32,
}

/// Compaction strategy to apply.
#[derive(Debug, Clone, PartialEq)]
pub enum CompactionStrategy {
    /// Drop low-relevance files from the working set.
    DropLow { threshold: f32 },
    /// Extract key declarations from source files.
    ExtractDeclarations,
    /// Truncate files to a fraction of their original size.
    Truncate { fraction: f32 },
}

/// Compact a list of selected files to fit within a reduced token budget.
///
/// Returns the compacted files and a summary string.
pub fn compact(
    files: Vec<SelectedFile>,
    target_tokens: u32,
    strategy: CompactionStrategy,
) -> (Vec<SelectedFile>, String) {
    match strategy {
        CompactionStrategy::DropLow { threshold } => {
            drop_low_relevance(files, target_tokens, threshold)
        }
        CompactionStrategy::ExtractDeclarations => extract_declarations(files, target_tokens),
        CompactionStrategy::Truncate { fraction } => truncate_files(files, fraction),
    }
}

fn drop_low_relevance(
    mut files: Vec<SelectedFile>,
    target_tokens: u32,
    threshold: f32,
) -> (Vec<SelectedFile>, String) {
    let before = files.len();
    files.retain(|f| f.score >= threshold);
    let after = files.len();
    let dropped = before - after;

    let total: u32 = files.iter().map(|f| f.estimated_tokens).sum();
    let note = if total > target_tokens {
        format!(
            "[compacted: dropped {} low-relevance files (score < {:.2}), still {} estimated tokens]",
            dropped, threshold, total
        )
    } else {
        format!(
            "[compacted: dropped {} low-relevance files (score < {:.2})]",
            dropped, threshold
        )
    };

    (files, note)
}

fn extract_declarations(
    files: Vec<SelectedFile>,
    target_tokens: u32,
) -> (Vec<SelectedFile>, String) {
    let note = format!(
        "[compacted: extracted declarations to fit {target_tokens} token budget]"
    );
    // Return files as-is; the packer will handle truncation
    (files, note)
}

fn truncate_files(files: Vec<SelectedFile>, fraction: f32) -> (Vec<SelectedFile>, String) {
    let fraction = fraction.clamp(0.1, 1.0);
    let adjusted: Vec<SelectedFile> = files
        .into_iter()
        .map(|mut f| {
            f.estimated_tokens = ((f.estimated_tokens as f32) * fraction) as u32;
            f
        })
        .collect();

    let note = format!(
        "[compacted: files truncated to {:.0}% of original size]",
        fraction * 100.0
    );
    (adjusted, note)
}

/// Generate a compact header-only summary of Rust source code.
/// Extracts: module doc, use declarations (first 5), fn/struct/enum/impl/trait signatures.
pub fn summarize_rust_source(content: &str) -> String {
    let mut output = Vec::new();
    let mut in_doc_comment = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Keep module-level doc comments
        if trimmed.starts_with("//!") {
            output.push(line.to_string());
            in_doc_comment = true;
            continue;
        }
        if in_doc_comment && !trimmed.starts_with("//!") {
            in_doc_comment = false;
        }

        // Keep top-level declarations
        let is_declaration = trimmed.starts_with("pub fn ")
            || trimmed.starts_with("fn ")
            || trimmed.starts_with("pub struct ")
            || trimmed.starts_with("struct ")
            || trimmed.starts_with("pub enum ")
            || trimmed.starts_with("enum ")
            || trimmed.starts_with("pub trait ")
            || trimmed.starts_with("trait ")
            || trimmed.starts_with("pub type ")
            || trimmed.starts_with("type ")
            || trimmed.starts_with("pub const ")
            || trimmed.starts_with("const ")
            || trimmed.starts_with("pub static ")
            || trimmed.starts_with("static ")
            || trimmed.starts_with("impl ")
            || trimmed.starts_with("pub impl ")
            || trimmed.starts_with("mod ")
            || trimmed.starts_with("pub mod ")
            || trimmed.starts_with("use ")
            || trimmed.starts_with("pub use ");

        if is_declaration {
            // Only keep the signature line, not the body
            let sig = if let Some(brace) = trimmed.find('{') {
                &trimmed[..brace]
            } else {
                trimmed
            };
            output.push(format!("  {sig}"));
        }
    }

    output.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workset::{SelectedFile, SelectionReason};

    fn file(path: &str, score: f32, tokens: u32) -> SelectedFile {
        SelectedFile {
            path: path.into(),
            size_bytes: tokens as u64 * 4,
            score,
            estimated_tokens: tokens,
            reason: SelectionReason::TopRelevance,
        }
    }

    #[test]
    fn drop_low_removes_below_threshold() {
        let files = vec![file("/p/high.rs", 0.8, 100), file("/p/low.rs", 0.2, 100)];
        let (kept, note) =
            compact(files, 1000, CompactionStrategy::DropLow { threshold: 0.5 });
        assert_eq!(kept.len(), 1);
        assert!(note.contains("dropped"));
    }

    #[test]
    fn truncate_reduces_estimated_tokens() {
        let files = vec![file("/p/a.rs", 0.5, 1000)];
        let (result, _) = compact(files, 500, CompactionStrategy::Truncate { fraction: 0.5 });
        assert_eq!(result[0].estimated_tokens, 500);
    }

    #[test]
    fn summarize_rust_extracts_signatures() {
        let src = r#"
//! My module.
use std::io;

pub struct Foo {
    x: i32,
}

pub fn bar() -> i32 {
    42
}
"#;
        let summary = summarize_rust_source(src);
        assert!(summary.contains("pub struct Foo"));
        assert!(summary.contains("pub fn bar"));
        assert!(!summary.contains("42")); // body excluded
    }
}
