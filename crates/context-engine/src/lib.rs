//! Context engine for building prompt context from working directories.
//!
//! # Pipeline
//! 1. [`scanner::Scanner`] discovers candidate files
//! 2. [`ignore::IgnoreEngine`] filters excluded paths
//! 3. [`relevance`] scores files for the current task
//! 4. [`workset`] selects a bounded working set
//! 5. [`memory`] retrieves relevant memory snippets
//! 6. [`packer::ContextPacker`] assembles token-bounded context
//! 7. [`compactor`] reduces context when budget is exceeded
//!
//! Entry point: [`engine::ContextEngine`]

pub mod compactor;
pub mod engine;
pub mod error;
pub mod ignore;
pub mod memory;
pub mod packer;
pub mod relevance;
pub mod scanner;
pub mod tokens;
pub mod workset;

pub use engine::{ContextEngine, EngineConfig, EngineStats};
pub use error::ContextError;
pub use packer::PackedContext;
pub use relevance::RelevanceHints;
