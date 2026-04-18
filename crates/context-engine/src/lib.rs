//! Context engine for building prompt context from working directories.
//!
//! Responsibilities:
//! - Scan the filesystem for relevant files
//! - Apply `.gitignore` and tool-specific ignore rules
//! - Score file relevance
//! - Pack files into a token budget
//! - Compact/summarize context when budgets are exceeded
//!
//! Phase 0 stub — full implementation deferred to Phase 6.

pub mod error;
pub mod packer;
pub mod scanner;
