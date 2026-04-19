//! Library façade for the `cli` crate.
//!
//! Exposes internal modules so integration tests can exercise the `Executor`,
//! `Output`, and args types directly without going through the binary.

pub mod args;
pub mod commands;
pub mod error;
pub mod executor;
pub mod output;
