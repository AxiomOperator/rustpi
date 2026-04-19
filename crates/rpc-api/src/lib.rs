//! JSONL RPC protocol over stdin/stdout.
//!
//! All requests and responses are single JSON objects, one per line.
//! The server emits a stream of [`RpcEvent`]s after each command.
//!
//! # Protocol
//! - Client writes one [`RpcRequest`] per line to stdin
//! - Server writes [`RpcResponse`] or streaming [`RpcEvent`]s to stdout
//! - Each line is a complete JSON object (JSONL)

pub mod dispatch;
pub mod error;
pub mod normalize;
pub mod protocol;
pub mod provider_factory;
pub mod server;
pub mod transport;

pub use error::RpcError;
pub use protocol::*;
pub use server::{RpcServer, ServerState};
pub use transport::{LineReader, LineWriter};

/// Build an `RpcServer` reading from stdin and writing to stdout.
pub fn stdio_server() -> RpcServer<tokio::io::Stdin, tokio::io::Stdout> {
    RpcServer::new(tokio::io::stdin(), tokio::io::stdout())
}
