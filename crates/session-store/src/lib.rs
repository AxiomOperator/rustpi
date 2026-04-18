//! Session and run persistence layer.
//!
//! Provides pluggable backends behind trait interfaces:
//! - SQLite (default, local)
//! - sled (embedded key-value)
//! - PostgreSQL (production)
//!
//! Phase 0 stub — backend implementations deferred to Phase 7.

pub mod error;
pub mod store;

pub use error::StoreError;
