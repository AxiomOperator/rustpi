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
}

#[async_trait]
impl MemoryRetriever for VaultMemory {
    async fn retrieve(&self, query: &MemoryQuery) -> Vec<MemorySnippet> {
        let mut snippets = Vec::new();
        let mut total_tokens = 0u32;

        let entries = match std::fs::read_dir(&self.vault_root) {
            Ok(e) => e,
            Err(_) => return snippets,
        };

        let mut paths: Vec<_> = entries
            .flatten()
            .filter(|e| {
                e.path()
                    .extension()
                    .and_then(|x| x.to_str())
                    .map(|x| x == "md")
                    .unwrap_or(false)
            })
            .map(|e| e.path())
            .collect();
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

            let source = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();

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
                        || source.to_lowercase().contains(&kw.to_lowercase())
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
