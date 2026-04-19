//! Tool execution runtime.
//!
//! All tools execute through a unified runtime that enforces:
//! - Timeouts
//! - Cancellation via `tokio::CancellationToken`
//! - Approval hooks for sensitive operations
//! - Per-event streaming (stdout/stderr)
//! - Path traversal protection
//!
//! # Tool registration
//! Tools are registered by name with a JSON Schema describing their parameters.
//! The model receives these schemas and requests tool calls by name.
//!
//! Phase 0 stub — subprocess execution and file tools deferred to Phase 5.

pub mod approval;
pub mod audit;
pub mod error;
pub mod overwrite_policy;
pub mod path_safety;
pub mod registry;
pub mod runner;
pub mod schema;
pub mod subprocess;
pub mod tools;

pub use agent_core::types::{ToolCall, ToolResult};
pub use audit::AuditSink;
pub use error::ToolError;
pub use overwrite_policy::OverwritePolicy;
pub use tools::shell::ShellTool;
