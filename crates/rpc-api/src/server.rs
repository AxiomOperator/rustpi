//! RPC server: reads requests, dispatches, writes responses.

use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use agent_core::bus::EventBus;
use agent_core::run::Run;
use agent_core::session::Session;
use agent_core::types::{RunId, SessionId};
use auth_core::{MemoryTokenStore, TokenStore};
use policy_engine::PolicyEngine;
use event_log::store::EventStore;
use model_adapters::ProviderRegistry;
use session_store::store::{RunStore, SessionStore};
use tool_runtime::{
    path_safety::PathSafetyPolicy,
    registry::ToolRegistry,
    runner::ToolRunner,
    tools::{
        edit::EditTool,
        file::{ReadFileTool, WriteFileTool},
        shell::ShellTool,
    },
};
use tokio_util::sync::CancellationToken;

use crate::dispatch;
use crate::error::RpcError;
use crate::protocol::{RpcErrorCode, RpcRequest, RpcResponse};
use crate::transport::{LineReader, LineWriter};

/// Shared mutable state for the RPC server.
pub struct ServerState {
    pub sessions: Mutex<HashMap<SessionId, Session>>,
    pub runs: Mutex<HashMap<RunId, Run>>,
    pub event_bus: EventBus,
    pub next_seq: AtomicU64,
    pub cancel_tokens: Mutex<HashMap<RunId, CancellationToken>>,
    pub provider_registry: Arc<ProviderRegistry>,
    pub token_store: Arc<dyn TokenStore>,
    pub session_store: Option<Arc<dyn SessionStore>>,
    pub run_store: Option<Arc<dyn RunStore>>,
    pub event_store: Option<Arc<dyn EventStore>>,
    pub tool_runner: Arc<ToolRunner>,
    pub policy_engine: Arc<PolicyEngine>,
}

fn default_tool_runner() -> Arc<ToolRunner> {
    let policy = Arc::new(PathSafetyPolicy::allow_all());
    let mut registry = ToolRegistry::default();
    registry.register(Arc::new(ShellTool::new()));
    registry.register(Arc::new(ReadFileTool::new(Arc::clone(&policy))));
    registry.register(Arc::new(WriteFileTool::new(Arc::clone(&policy))));
    registry.register(Arc::new(EditTool::new(Arc::clone(&policy))));
    Arc::new(ToolRunner::new(Arc::new(registry), std::time::Duration::from_secs(30)))
}

impl ServerState {
    pub fn new() -> Arc<Self> {
        Self::with_registry(ProviderRegistry::new())
    }

    pub fn with_registry(registry: ProviderRegistry) -> Arc<Self> {
        Self::with_registry_and_store(registry, Arc::new(MemoryTokenStore::new()))
    }

    pub fn with_registry_and_store(
        registry: ProviderRegistry,
        token_store: Arc<dyn TokenStore>,
    ) -> Arc<Self> {
        Arc::new(Self {
            sessions: Mutex::new(HashMap::new()),
            runs: Mutex::new(HashMap::new()),
            event_bus: EventBus::new(),
            next_seq: AtomicU64::new(0),
            cancel_tokens: Mutex::new(HashMap::new()),
            provider_registry: Arc::new(registry),
            token_store,
            session_store: None,
            run_store: None,
            event_store: None,
            tool_runner: default_tool_runner(),
            policy_engine: Arc::new(PolicyEngine::default()),
        })
    }

    pub fn new_with_all(
        registry: ProviderRegistry,
        token_store: Arc<dyn TokenStore>,
        session_store: Arc<dyn SessionStore>,
        run_store: Arc<dyn RunStore>,
        event_store: Arc<dyn EventStore>,
    ) -> Arc<Self> {
        let state = Arc::new(Self {
            sessions: Mutex::new(HashMap::new()),
            runs: Mutex::new(HashMap::new()),
            event_bus: EventBus::new(),
            next_seq: AtomicU64::new(0),
            cancel_tokens: Mutex::new(HashMap::new()),
            provider_registry: Arc::new(registry),
            token_store,
            session_store: Some(session_store),
            run_store: Some(run_store),
            event_store: Some(event_store.clone()),
            tool_runner: default_tool_runner(),
            policy_engine: Arc::new(PolicyEngine::default()),
        });

        // Spawn background task to persist events from the event bus.
        let event_store_bg = event_store;
        let mut bus_rx = state.event_bus.subscribe();
        tokio::spawn(async move {
            while let Ok(event) = bus_rx.recv().await {
                if let Err(e) = event_store_bg.append(&event).await {
                    tracing::warn!("event persistence failed: {}", e);
                }
            }
        });

        state
    }
}

/// The RPC server. Reads requests from a `LineReader`, dispatches each, writes responses.
pub struct RpcServer<R, W> {
    reader: LineReader<R>,
    writer: LineWriter<W>,
    state: Arc<ServerState>,
}

impl<R, W> RpcServer<R, W>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            reader: LineReader::new(reader),
            writer: LineWriter::new(writer),
            state: ServerState::new(),
        }
    }

    /// Build a server using a pre-constructed [`ServerState`] (e.g. for tests with mock providers).
    pub fn with_state(state: Arc<ServerState>, reader: R, writer: W) -> Self {
        Self {
            reader: LineReader::new(reader),
            writer: LineWriter::new(writer),
            state,
        }
    }

    /// Run the server loop: read requests until EOF, dispatch each, write responses.
    pub async fn run(mut self) -> Result<(), RpcError> {
        loop {
            match self.reader.next::<RpcRequest>().await {
                None => break,
                Some(Err(e)) => {
                    let resp = RpcResponse::Error {
                        request_id: "unknown".to_string(),
                        code: RpcErrorCode::ParseError.to_string(),
                        message: e.to_string(),
                    };
                    match self.writer.write(&resp).await {
                        Ok(()) => {}
                        Err(RpcError::BrokenPipe) => break,
                        Err(_) => break,
                    }
                }
                Some(Ok(request)) => {
                    let result =
                        dispatch::dispatch(&request, &self.state, &self.writer).await;
                    match result {
                        Ok(()) => {}
                        Err(RpcError::BrokenPipe) => break,
                        Err(e) => {
                            // Write internal error response, best-effort
                            let resp = RpcResponse::Error {
                                request_id: request.id.clone(),
                                code: RpcErrorCode::InternalError.to_string(),
                                message: e.to_string(),
                            };
                            let _ = self.writer.write(&resp).await;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{RpcMethod, RpcRequest, RpcResponse};
    use agent_core::types::ProviderId;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    async fn run_server_with_input(input: &str) -> String {
        let (mut client_write, server_read) = tokio::io::duplex(8192);
        let (server_write, client_read) = tokio::io::duplex(8192);

        client_write.write_all(input.as_bytes()).await.unwrap();
        drop(client_write);

        let server = RpcServer::new(server_read, server_write);
        server.run().await.unwrap();

        let mut buf = String::new();
        let mut reader = BufReader::new(client_read);
        reader.read_line(&mut buf).await.unwrap();
        buf
    }

    #[tokio::test]
    async fn test_capabilities_request() {
        let request = RpcRequest {
            id: "req-1".into(),
            method: RpcMethod::Capabilities { provider: ProviderId::new("openai") },
        };
        let input = serde_json::to_string(&request).unwrap() + "\n";

        let (mut client_write, server_read) = tokio::io::duplex(4096);
        let (server_write, client_read) = tokio::io::duplex(4096);

        client_write.write_all(input.as_bytes()).await.unwrap();
        drop(client_write);

        let server = RpcServer::new(server_read, server_write);
        server.run().await.unwrap();

        let mut buf = String::new();
        let mut reader = BufReader::new(client_read);
        reader.read_line(&mut buf).await.unwrap();
        let resp: RpcResponse = serde_json::from_str(buf.trim()).unwrap();
        assert!(matches!(resp, RpcResponse::Success { .. }));
    }

    #[tokio::test]
    async fn test_invalid_json_returns_parse_error() {
        let line = run_server_with_input("not valid json\n").await;
        let resp: RpcResponse = serde_json::from_str(line.trim()).unwrap();
        match resp {
            RpcResponse::Error { code, .. } => assert_eq!(code, "parse_error"),
            other => panic!("expected Error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_session_join_no_id_creates_session() {
        let request = RpcRequest {
            id: "req-2".into(),
            method: RpcMethod::SessionAttach { session_id: None },
        };
        let input = serde_json::to_string(&request).unwrap() + "\n";
        let line = {
            let (mut client_write, server_read) = tokio::io::duplex(4096);
            let (server_write, client_read) = tokio::io::duplex(4096);
            client_write.write_all(input.as_bytes()).await.unwrap();
            drop(client_write);
            let server = RpcServer::new(server_read, server_write);
            server.run().await.unwrap();
            let mut buf = String::new();
            let mut reader = BufReader::new(client_read);
            reader.read_line(&mut buf).await.unwrap();
            buf
        };
        let resp: RpcResponse = serde_json::from_str(line.trim()).unwrap();
        assert!(matches!(resp, RpcResponse::Success { .. }));
    }
}
