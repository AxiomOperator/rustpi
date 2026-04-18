//! JSONL RPC protocol over stdin/stdout.
//!
//! All requests and responses are single JSON objects, one per line.
//! The server emits a stream of [`RpcEvent`]s after each command.
//!
//! # Protocol
//! - Client writes one [`RpcRequest`] per line to stdin
//! - Server writes [`RpcResponse`] or streaming [`RpcEvent`]s to stdout
//! - Each line is a complete JSON object (JSONL)
//!
//! Phase 0 stub — framing and dispatch deferred to Phase 9.

pub mod error;
pub mod protocol;
