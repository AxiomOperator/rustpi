//! Context packer: assembles files into a token-bounded context string.
//!
//! Phase 0 stub.

use crate::scanner::ScoredFile;

/// A packed context ready to be inserted into a prompt.
#[derive(Debug, Clone)]
pub struct PackedContext {
    pub content: String,
    pub token_count: u32,
    pub included_files: Vec<std::path::PathBuf>,
    pub excluded_files: Vec<std::path::PathBuf>,
}

pub struct ContextPacker {
    pub token_budget: u32,
}

impl ContextPacker {
    pub fn new(token_budget: u32) -> Self {
        Self { token_budget }
    }

    /// Pack scored files into context up to the token budget.
    ///
    /// Phase 0 stub — returns empty context.
    pub fn pack(&self, _files: Vec<ScoredFile>) -> PackedContext {
        PackedContext {
            content: String::new(),
            token_count: 0,
            included_files: vec![],
            excluded_files: vec![],
        }
    }
}
