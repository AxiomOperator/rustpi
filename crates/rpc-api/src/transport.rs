//! JSONL framing over async readers and writers.

use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

use crate::error::RpcError;

/// JSONL line reader from an async reader (e.g. stdin).
pub struct LineReader<R> {
    inner: BufReader<R>,
}

impl<R: tokio::io::AsyncRead + Unpin> LineReader<R> {
    pub fn new(reader: R) -> Self {
        Self { inner: BufReader::new(reader) }
    }

    /// Read the next line and deserialize as T.
    /// Returns `None` on EOF, `Err` on IO error or parse failure, skips empty lines.
    pub async fn next<T: serde::de::DeserializeOwned>(&mut self) -> Option<Result<T, RpcError>> {
        loop {
            let mut line = String::new();
            match self.inner.read_line(&mut line).await {
                Ok(0) => return None,
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    return Some(serde_json::from_str(trimmed).map_err(RpcError::from));
                }
                Err(e) => return Some(Err(RpcError::Io(e))),
            }
        }
    }
}

/// JSONL line writer to an async writer (e.g. stdout).
///
/// Internally uses `Arc<Mutex<W>>` so it can be cheaply cloned and shared across tasks
/// without interleaving writes.
pub struct LineWriter<W> {
    inner: Arc<Mutex<W>>,
}

impl<W: tokio::io::AsyncWrite + Unpin + Send> LineWriter<W> {
    pub fn new(writer: W) -> Self {
        Self { inner: Arc::new(Mutex::new(writer)) }
    }

    /// Serialize `value` to JSON and write as a single line followed by `\n`.
    /// Acquires the write lock to prevent interleaving from concurrent tasks.
    pub async fn write<T: serde::Serialize>(&self, value: &T) -> Result<(), RpcError> {
        let mut json = serde_json::to_string(value)?;
        json.push('\n');
        let mut guard = self.inner.lock().await;
        guard.write_all(json.as_bytes()).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::BrokenPipe {
                RpcError::BrokenPipe
            } else {
                RpcError::Io(e)
            }
        })?;
        guard.flush().await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::BrokenPipe {
                RpcError::BrokenPipe
            } else {
                RpcError::Io(e)
            }
        })
    }
}

impl<W> Clone for LineWriter<W> {
    fn clone(&self) -> Self {
        Self { inner: Arc::clone(&self.inner) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn test_reader_valid_json() {
        let data = b"{\"hello\":\"world\"}\n";
        let (mut w, r) = tokio::io::duplex(4096);
        w.write_all(data).await.unwrap();
        drop(w);
        let mut reader = LineReader::new(r);
        let result: Option<Result<serde_json::Value, _>> = reader.next().await;
        let val = result.unwrap().unwrap();
        assert_eq!(val["hello"], "world");
    }

    #[tokio::test]
    async fn test_reader_skips_empty_lines() {
        let data = b"\n\n{\"x\":1}\n";
        let (mut w, r) = tokio::io::duplex(4096);
        w.write_all(data).await.unwrap();
        drop(w);
        let mut reader = LineReader::new(r);
        let result: Option<Result<serde_json::Value, _>> = reader.next().await;
        let val = result.unwrap().unwrap();
        assert_eq!(val["x"], 1);
    }

    #[tokio::test]
    async fn test_reader_invalid_json_returns_err() {
        let data = b"not valid json\n";
        let (mut w, r) = tokio::io::duplex(4096);
        w.write_all(data).await.unwrap();
        drop(w);
        let mut reader = LineReader::new(r);
        let result: Option<Result<serde_json::Value, _>> = reader.next().await;
        assert!(result.unwrap().is_err());
    }

    #[tokio::test]
    async fn test_reader_eof_returns_none() {
        let (w, r) = tokio::io::duplex(4096);
        drop(w);
        let mut reader = LineReader::new(r);
        let result: Option<Result<serde_json::Value, _>> = reader.next().await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_writer_adds_newline() {
        let (w, mut r) = tokio::io::duplex(4096);
        let writer = LineWriter::new(w);
        writer.write(&json!({"test": true})).await.unwrap();
        drop(writer);

        let mut buf = String::new();
        tokio::io::AsyncBufReadExt::read_line(&mut tokio::io::BufReader::new(&mut r), &mut buf)
            .await
            .unwrap();
        assert!(buf.ends_with('\n'));
        let val: serde_json::Value = serde_json::from_str(buf.trim()).unwrap();
        assert_eq!(val["test"], true);
    }

    #[tokio::test]
    async fn test_roundtrip() {
        use crate::protocol::{RpcMethod, RpcRequest};
        use agent_core::types::ProviderId;

        let req = RpcRequest {
            id: "rt-1".into(),
            method: RpcMethod::Capabilities { provider: ProviderId::new("openai") },
        };

        let (w, r) = tokio::io::duplex(4096);
        let writer = LineWriter::new(w);
        writer.write(&req).await.unwrap();
        drop(writer);

        let mut reader = LineReader::new(r);
        let result: Option<Result<RpcRequest, _>> = reader.next().await;
        let decoded = result.unwrap().unwrap();
        assert_eq!(decoded.id, "rt-1");
    }
}
