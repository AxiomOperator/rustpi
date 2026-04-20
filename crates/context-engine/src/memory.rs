//! Memory retrieval hooks for context assembly.
//!
//! Provides a trait-based contract for retrieving relevant memory snippets
//! into the context window. The concrete implementation is deferred to
//! Phase 8 (memory-sync), but this hook ensures the context engine can
//! pull memory cleanly without coupling to implementation details.
//!
//! # Memory budget
//! Memory competes with file context under the same token budget or a
//! clearly defined sub-budget (controlled by `PackerConfig::memory_budget`).

use crate::packer::MemorySnippet;
use crate::tokens;
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;

/// A query for memory retrieval.
#[derive(Debug, Clone)]
pub struct MemoryQuery {
    /// Free-text keywords from the current task.
    pub keywords: Vec<String>,
    /// Session ID for filtering session-specific memory.
    pub session_id: Option<String>,
    /// Maximum number of snippets to retrieve.
    pub max_snippets: usize,
    /// Maximum tokens per snippet.
    pub max_tokens_per_snippet: u32,
    /// Total token budget for all retrieved memory.
    pub total_token_budget: u32,
}

impl Default for MemoryQuery {
    fn default() -> Self {
        Self {
            keywords: vec![],
            session_id: None,
            max_snippets: 5,
            max_tokens_per_snippet: 512,
            total_token_budget: 2_000,
        }
    }
}

/// Trait for memory backends that can provide context-relevant snippets.
///
/// Implementations must be async and token-aware.
#[async_trait]
pub trait MemoryRetriever: Send + Sync {
    /// Retrieve relevant memory snippets for the given query.
    async fn retrieve(&self, query: &MemoryQuery) -> Vec<MemorySnippet>;
}

/// A no-op memory retriever. Used when no memory backend is configured.
/// Phase 8 will replace this with a real vault/store-backed implementation.
pub struct NoopMemory;

#[async_trait]
impl MemoryRetriever for NoopMemory {
    async fn retrieve(&self, _query: &MemoryQuery) -> Vec<MemorySnippet> {
        vec![]
    }
}

/// A static memory retriever for testing and simple configurations.
/// Holds pre-loaded snippets and returns the most relevant ones.
pub struct StaticMemory {
    snippets: Vec<MemorySnippet>,
}

impl StaticMemory {
    pub fn new(snippets: Vec<MemorySnippet>) -> Self {
        Self { snippets }
    }
}

#[async_trait]
impl MemoryRetriever for StaticMemory {
    async fn retrieve(&self, query: &MemoryQuery) -> Vec<MemorySnippet> {
        let mut result = Vec::new();
        let mut total_tokens = 0u32;

        for snip in &self.snippets {
            if result.len() >= query.max_snippets {
                break;
            }
            if total_tokens + snip.tokens > query.total_token_budget {
                break;
            }
            if snip.tokens > query.max_tokens_per_snippet {
                continue;
            }

            // Simple keyword matching for relevance
            let matches = query.keywords.is_empty()
                || query.keywords.iter().any(|kw| {
                    snip.content.to_lowercase().contains(&kw.to_lowercase())
                        || snip.source.to_lowercase().contains(&kw.to_lowercase())
                });

            if matches {
                total_tokens += snip.tokens;
                result.push(snip.clone());
            }
        }

        result
    }
}

/// Vault memory retriever: reads Markdown files from a vault directory.
/// Phase 8 stub — loads all vault files and returns them as snippets.
pub struct VaultMemory {
    vault_root: PathBuf,
}

impl VaultMemory {
    pub fn new(vault_root: impl AsRef<std::path::Path>) -> Self {
        Self { vault_root: vault_root.as_ref().to_path_buf() }
    }

    /// Recursively collect all `.md` file paths under `dir`.
    fn collect_md_files(dir: &PathBuf) -> Vec<PathBuf> {
        let mut result = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else { return result };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                result.extend(Self::collect_md_files(&path));
            } else if path.extension().and_then(|x| x.to_str()) == Some("md") {
                result.push(path);
            }
        }
        result.sort();
        result
    }
}

#[async_trait]
impl MemoryRetriever for VaultMemory {
    async fn retrieve(&self, query: &MemoryQuery) -> Vec<MemorySnippet> {
        let mut snippets = Vec::new();
        let mut total_tokens = 0u32;

        let mut paths = Self::collect_md_files(&self.vault_root);
        paths.sort();

        for path in paths {
            if snippets.len() >= query.max_snippets {
                break;
            }
            if total_tokens >= query.total_token_budget {
                break;
            }

            let content = match tokio::fs::read_to_string(&path).await {
                Ok(c) => c,
                Err(_) => continue,
            };

            // Source: "vault/{relative/path.md}" for structured provenance
            let rel = path.strip_prefix(&self.vault_root)
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown").to_string());
            let source = format!("vault/{rel}");

            let snip_tokens = tokens::estimate(&content);
            let (final_content, final_tokens) = if snip_tokens > query.max_tokens_per_snippet {
                let max_chars = (query.max_tokens_per_snippet as usize) * 4;
                let trimmed = content[..max_chars.min(content.len())].to_string();
                let t = tokens::estimate(&trimmed);
                (trimmed, t)
            } else {
                (content, snip_tokens)
            };

            let matches = query.keywords.is_empty()
                || query.keywords.iter().any(|kw| {
                    final_content.to_lowercase().contains(&kw.to_lowercase())
                        || rel.to_lowercase().contains(&kw.to_lowercase())
                });

            if matches {
                total_tokens += final_tokens;
                snippets.push(MemorySnippet {
                    source,
                    content: final_content,
                    tokens: final_tokens,
                });
            }
        }

        snippets
    }
}

/// A memory retriever that combines multiple backends.
/// Results are interleaved in round-robin order (one from each backend in turn)
/// to prevent any single backend from dominating, with per-snippet deduplication
/// and global token budget enforcement.
pub struct CombinedMemory {
    retrievers: Vec<Arc<dyn MemoryRetriever>>,
}

impl CombinedMemory {
    pub fn new(retrievers: Vec<Arc<dyn MemoryRetriever>>) -> Self {
        Self { retrievers }
    }
}

#[async_trait]
impl MemoryRetriever for CombinedMemory {
    async fn retrieve(&self, query: &MemoryQuery) -> Vec<MemorySnippet> {
        // Fan-out all retrievers concurrently.
        let per_source_budget = query.total_token_budget / self.retrievers.len().max(1) as u32;
        let sub_query = MemoryQuery {
            total_token_budget: per_source_budget,
            ..query.clone()
        };
        let mut all_lists: Vec<Vec<MemorySnippet>> = Vec::with_capacity(self.retrievers.len());
        for r in self.retrievers.iter() {
            let list: Vec<MemorySnippet> = r.retrieve(&sub_query).await;
            all_lists.push(list);
        }

        // Round-robin interleave: one from each source in turn.
        let mut result: Vec<MemorySnippet> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut total_tokens = 0u32;
        let max_idx = all_lists.iter().map(|l| l.len()).max().unwrap_or(0);

        for i in 0..max_idx {
            for list in &all_lists {
                if let Some(snip) = list.get(i) {
                    if result.len() >= query.max_snippets { break; }
                    if total_tokens + snip.tokens > query.total_token_budget { continue; }
                    // Dedupe by content prefix
                    let key = snip.content.chars().take(80).collect::<String>();
                    if seen.insert(key) {
                        total_tokens += snip.tokens;
                        result.push(snip.clone());
                    }
                }
            }
            if result.len() >= query.max_snippets { break; }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snippet(source: &str, content: &str) -> MemorySnippet {
        let tokens = tokens::estimate(content);
        MemorySnippet { source: source.into(), content: content.into(), tokens }
    }

    #[tokio::test]
    async fn noop_returns_empty() {
        let mem = NoopMemory;
        let result = mem.retrieve(&MemoryQuery::default()).await;
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn static_returns_matching_snippets() {
        let mem = StaticMemory::new(vec![
            snippet("agents.md", "This is about agents and tools."),
            snippet("boot.md", "Bootstrap instructions here."),
        ]);
        let query = MemoryQuery {
            keywords: vec!["agent".to_string()],
            ..Default::default()
        };
        let result = mem.retrieve(&query).await;
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].source, "agents.md");
    }

    #[tokio::test]
    async fn static_respects_budget() {
        let mem = StaticMemory::new(vec![
            snippet("a.md", &"a".repeat(1000)),
            snippet("b.md", &"b".repeat(1000)),
        ]);
        let query = MemoryQuery {
            total_token_budget: 100,
            keywords: vec![],
            ..Default::default()
        };
        let result = mem.retrieve(&query).await;
        let total: u32 = result.iter().map(|s| s.tokens).sum();
        assert!(total <= 100);
    }
}
