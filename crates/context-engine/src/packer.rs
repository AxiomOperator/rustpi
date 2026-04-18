//! Context packing: assembles working-set files into token-bounded context blocks.
//!
//! Reads file contents and packs them into structured context sections that
//! can be directly passed to `agent_core::prompt::PromptAssembler`.
//!
//! # Truncation strategy
//! When a file's content would exceed the remaining budget, the file is
//! truncated at a line boundary and a `[truncated]` marker is appended.

use crate::{
    error::ContextError,
    tokens::{self, Budget},
    workset::SelectedFile,
};
use std::path::PathBuf;

/// A single packed file block ready for prompt insertion.
#[derive(Debug, Clone)]
pub struct FileBlock {
    pub path: PathBuf,
    pub content: String,
    pub actual_tokens: u32,
    pub truncated: bool,
}

/// The result of packing a working set into prompt context.
#[derive(Debug, Clone)]
pub struct PackedContext {
    /// Packed file blocks, in priority order.
    pub blocks: Vec<FileBlock>,
    /// Memory snippets retrieved for this context.
    pub memory_snippets: Vec<MemorySnippet>,
    /// Compacted summary if some content was compacted.
    pub compact_summary: Option<String>,
    /// Total tokens used across all blocks and memory.
    pub total_tokens: u32,
    /// Whether any content was truncated to fit the budget.
    pub truncated: bool,
    /// Whether any content was compacted/summarized.
    pub compacted: bool,
    /// Files fully included.
    pub included_files: Vec<PathBuf>,
    /// Files excluded (didn't fit).
    pub excluded_files: Vec<PathBuf>,
}

/// A retrieved memory snippet.
#[derive(Debug, Clone)]
pub struct MemorySnippet {
    pub source: String,
    pub content: String,
    pub tokens: u32,
}

impl PackedContext {
    /// Render the full context as a single string for prompt injection.
    ///
    /// Format: fenced code blocks per file, then memory, then summary.
    pub fn render(&self) -> String {
        let mut out = String::new();

        for block in &self.blocks {
            let path_str = block.path.display();
            let ext = block.path.extension().and_then(|e| e.to_str()).unwrap_or("");
            out.push_str(&format!(
                "```{ext}\n// File: {path_str}\n{}\n```\n\n",
                block.content
            ));
        }

        for mem in &self.memory_snippets {
            out.push_str(&format!(
                "<!-- memory: {} -->\n{}\n\n",
                mem.source, mem.content
            ));
        }

        if let Some(summary) = &self.compact_summary {
            out.push_str(&format!("<!-- compacted summary -->\n{}\n\n", summary));
        }

        out
    }
}

/// Configuration for context packing.
#[derive(Debug, Clone)]
pub struct PackerConfig {
    /// Total token budget for file content.
    pub token_budget: u32,
    /// Token budget for memory snippets (within the total budget).
    pub memory_budget: u32,
    /// Maximum bytes to read from a single file before truncation.
    pub max_file_read_bytes: usize,
}

impl Default for PackerConfig {
    fn default() -> Self {
        Self {
            token_budget: 32_000,
            memory_budget: 4_000,
            max_file_read_bytes: 64 * 1024,
        }
    }
}

/// Packs a working set into a token-bounded context.
pub struct ContextPacker {
    config: PackerConfig,
}

impl ContextPacker {
    pub fn new(config: PackerConfig) -> Self {
        Self { config }
    }

    /// Pack selected files into context, respecting the token budget.
    pub async fn pack(
        &self,
        files: Vec<SelectedFile>,
        memory: Vec<MemorySnippet>,
    ) -> Result<PackedContext, ContextError> {
        let mut budget = Budget::new(self.config.token_budget);
        let mut blocks = Vec::new();
        let mut included = Vec::new();
        let mut excluded = Vec::new();
        let mut any_truncated = false;

        // Reserve space for memory snippets
        let mem_budget = self.config.memory_budget.min(budget.remaining());
        let mut mem_tokens_used = 0u32;
        let mut included_memory = Vec::new();

        for snip in &memory {
            if mem_tokens_used + snip.tokens <= mem_budget {
                mem_tokens_used += snip.tokens;
                included_memory.push(snip.clone());
            }
        }
        budget.consume(mem_tokens_used);

        // Pack files
        for file in files {
            if budget.is_exhausted() {
                excluded.push(file.path);
                continue;
            }

            let content_raw = match tokio::fs::read_to_string(&file.path).await {
                Ok(c) => c,
                Err(_) => {
                    excluded.push(file.path);
                    continue;
                }
            };

            // Truncate content to max_file_read_bytes
            let content_str = if content_raw.len() > self.config.max_file_read_bytes {
                &content_raw[..self.config.max_file_read_bytes]
            } else {
                &content_raw
            };

            let file_tokens = tokens::estimate(content_str);

            if budget.would_fit(file_tokens) {
                budget.consume(file_tokens);
                included.push(file.path.clone());
                blocks.push(FileBlock {
                    path: file.path,
                    content: content_str.to_string(),
                    actual_tokens: file_tokens,
                    truncated: content_raw.len() > self.config.max_file_read_bytes,
                });
            } else {
                // Truncate to remaining budget
                let remaining_tokens = budget.remaining();
                if remaining_tokens > 10 {
                    let max_chars = (remaining_tokens as usize) * 4;
                    let truncated_content = truncate_at_line_boundary(content_str, max_chars);
                    let truncated_tokens = tokens::estimate(&truncated_content);
                    budget.consume(truncated_tokens);
                    any_truncated = true;
                    included.push(file.path.clone());
                    let mut truncated_with_marker = truncated_content;
                    truncated_with_marker.push_str("\n... [truncated]");
                    blocks.push(FileBlock {
                        path: file.path,
                        content: truncated_with_marker,
                        actual_tokens: truncated_tokens,
                        truncated: true,
                    });
                } else {
                    excluded.push(file.path);
                }
            }
        }

        Ok(PackedContext {
            total_tokens: budget.used,
            blocks,
            memory_snippets: included_memory,
            compact_summary: None,
            truncated: any_truncated,
            compacted: false,
            included_files: included,
            excluded_files: excluded,
        })
    }
}

/// Truncate text at a line boundary to stay within max_chars.
fn truncate_at_line_boundary(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        return text.to_string();
    }
    let slice = &text[..max_chars];
    match slice.rfind('\n') {
        Some(pos) => slice[..pos].to_string(),
        None => slice.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workset::SelectedFile;

    #[test]
    fn truncate_at_boundary_respects_lines() {
        let text = "line1\nline2\nline3\n";
        let result = truncate_at_line_boundary(text, 10);
        // "line1\nline" = 10 chars, last newline at pos 5
        assert!(result.ends_with('\n') || !result.contains("line3"));
    }

    #[test]
    fn truncate_noop_when_short() {
        let text = "short text";
        assert_eq!(truncate_at_line_boundary(text, 1000), "short text");
    }

    #[test]
    fn render_includes_file_markers() {
        let ctx = PackedContext {
            blocks: vec![FileBlock {
                path: "src/main.rs".into(),
                content: "fn main() {}".into(),
                actual_tokens: 5,
                truncated: false,
            }],
            memory_snippets: vec![],
            compact_summary: None,
            total_tokens: 5,
            truncated: false,
            compacted: false,
            included_files: vec!["src/main.rs".into()],
            excluded_files: vec![],
        };
        let rendered = ctx.render();
        assert!(rendered.contains("// File: src/main.rs"));
        assert!(rendered.contains("fn main() {}"));
    }
}
