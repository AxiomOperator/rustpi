//! Policy engine for runtime guardrails.
//!
//! Evaluates allow/deny rules for:
//! - Tool execution (by name, argument patterns)
//! - File mutations (by path patterns)
//! - Provider selection
//! - Auth operations

pub mod decision;
pub mod error;
pub mod policy;
pub mod request;

pub use decision::PolicyDecision;
pub use error::PolicyError;
pub use policy::{DefaultPolicy, PolicyAction, PolicyEngine, PolicyRule, PolicyTarget, PolicyVerdict};
pub use request::{
    AuthAction, AuthRequest, FileOperation, FileMutationRequest, ProviderOperation,
    ProviderRequest, ToolRequest,
};

