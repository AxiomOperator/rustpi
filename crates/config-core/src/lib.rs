//! Layered configuration system.
//!
//! Precedence (lower overridden by higher):
//! `defaults` < `global` < `user` < `project` < `runtime overrides`
//!
//! Configuration is loaded from TOML files.

pub mod error;
pub mod loader;
pub mod model;

pub use loader::ConfigLoader;
pub use model::Config;
