//! Integration tests for the Phase 9 RPC API.
//!
//! Each test exercises the real `RpcServer` via `tokio::io::duplex()` pairs,
//! with a 5-second timeout to prevent hangs.

use std::time::Duration;

use async_trait::async_trait;
use futures::stream;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::time::timeout;

use agent_core::types::{AuthState, ModelId, ProviderId};
use model_adapters::{
    provider::{
        ChatMessage, CompletionRequest, CompletionResponse, EmbeddingRequest, EmbeddingResponse,
        FinishReason, MessageContent, ModelInfo, ModelProvider, ProviderMetadata, Role, TokenDelta,
        TokenUsage,
    },
    ProviderCapabilities, ProviderError,
};
use rpc_api::{
    AuthStatusInfo, CapabilitiesInfo, LineReader, LineWriter, RpcMethod, RpcRequest, RpcResponse,
    RpcServer, ServerState, SessionInfo,
};

use model_adapters::registry::ProviderRegistry;

const TIMEOUT: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------------
// Mock provider — returns a fixed stream of token deltas for unit tests
// ---------------------------------------------------------------------------

struct MockProvider {
    id: ProviderId,
    chunks: Vec<String>,
}

impl MockProvider {
    fn new(id: &str, chunks: Vec<&str>) -> Self {
        Self {
            id: ProviderId::new(id),
            chunks: chunks.into_iter().map(str::to_string).collect(),
        }
    }
}

#[async_trait]
impl ModelProvider for MockProvider {
    fn provider_id(&self) -> &ProviderId {
        &self.id
    }

    fn capabilities(&self, _model: &ModelId) -> ProviderCapabilities {
        ProviderCapabilities::default()
    }

    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            id: self.id.clone(),
            display_name: "Mock".to_string(),
            description: "test mock".to_string(),
            supported_auth_flows: vec![],
            requires_network: false,
        }
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        Ok(vec![])
    }

    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        Ok(CompletionResponse {
            message: ChatMessage {
                role: Role::Assistant,
                content: MessageContent::Text("ok".to_string()),
            },
            finish_reason: FinishReason::Stop,
            usage: TokenUsage { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 },
        })
    }

    async fn complete_stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<
        std::pin::Pin<
            Box<dyn futures::Stream<Item = Result<TokenDelta, ProviderError>> + Send>,
        >,
        ProviderError,
    > {
        let deltas: Vec<Result<TokenDelta, ProviderError>> = self
            .chunks
            .iter()
            .enumerate()
            .map(|(i, text)| {
                let is_last = i + 1 == self.chunks.len();
                Ok(TokenDelta {
                    text: Some(text.clone()),
                    tool_call: None,
                    finish_reason: if is_last { Some(FinishReason::Stop) } else { None },
                })
            })
            .collect();
        Ok(Box::pin(stream::iter(deltas)))
    }

    async fn embed(&self, _req: EmbeddingRequest) -> Result<EmbeddingResponse, ProviderError> {
        Err(ProviderError::UnsupportedCapability("embed".to_string()))
    }

    async fn auth_state(&self) -> AuthState {
        AuthState::Unauthenticated
    }
}

/// Build a `ServerState` pre-loaded with `MockProvider` using provider id `"default"`.
fn make_state_with_mock(chunks: Vec<&str>) -> std::sync::Arc<ServerState> {
    let mut registry = ProviderRegistry::new();
    registry.register(std::sync::Arc::new(MockProvider::new("default", chunks)));
    ServerState::with_registry(registry)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn send_req<W: AsyncWriteExt + Unpin>(writer: &mut W, req: &RpcRequest) {
    let mut line = serde_json::to_string(req).unwrap();
    line.push('\n');
    writer.write_all(line.as_bytes()).await.unwrap();
}

async fn read_resp<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> RpcResponse {
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    serde_json::from_str(line.trim())
        .unwrap_or_else(|e| panic!("failed to parse response: {e}\nraw line: {line:?}"))
}

/// Read responses until one whose `request_id` matches `id` is found.
/// Skips `StreamEvent` and `Event` responses silently.
async fn read_resp_for<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
    id: &str,
) -> RpcResponse {
    loop {
        let resp = read_resp(reader).await;
        if response_id(&resp).as_deref() == Some(id) {
            return resp;
        }
    }
}

fn response_id(resp: &RpcResponse) -> Option<String> {
    match resp {
        RpcResponse::Ack { request_id } => Some(request_id.clone()),
        RpcResponse::Success { request_id, .. } => Some(request_id.clone()),
        RpcResponse::Error { request_id, .. } => Some(request_id.clone()),
        _ => None,
    }
}

fn session_id_from_success(resp: &RpcResponse) -> String {
    match resp {
        RpcResponse::Success { data, .. } => {
            data["session_id"].as_str().expect("session_id field missing").to_string()
        }
        other => panic!("expected Success with session_id, got {:?}", other),
    }
}

fn parse_session_id(s: &str) -> agent_core::types::SessionId {
    serde_json::from_value(serde_json::Value::String(s.to_string())).unwrap()
}

// ---------------------------------------------------------------------------
// Transport layer — LineReader
// ---------------------------------------------------------------------------

#[tokio::test]
async fn transport_line_reader_single_line() {
    let (mut w, r) = tokio::io::duplex(4096);
    w.write_all(b"{\"x\":1}\n").await.unwrap();
    drop(w);

    let mut reader = LineReader::new(r);
    let result: Option<Result<serde_json::Value, _>> =
        timeout(TIMEOUT, reader.next()).await.unwrap();
    let val = result.unwrap().unwrap();
    assert_eq!(val["x"], 1);
}

#[tokio::test]
async fn transport_line_reader_multiple_lines() {
    let (mut w, r) = tokio::io::duplex(4096);
    w.write_all(b"{\"n\":1}\n{\"n\":2}\n{\"n\":3}\n").await.unwrap();
    drop(w);

    let mut reader = LineReader::new(r);
    for expected in [1i64, 2, 3] {
        let val: serde_json::Value =
            timeout(TIMEOUT, reader.next()).await.unwrap().unwrap().unwrap();
        assert_eq!(val["n"], expected, "line mismatch for n={expected}");
    }
    // EOF
    let eof: Option<Result<serde_json::Value, _>> =
        timeout(TIMEOUT, reader.next()).await.unwrap();
    assert!(eof.is_none(), "expected EOF after last line");
}

#[tokio::test]
async fn transport_line_reader_skips_blank_lines() {
    let (mut w, r) = tokio::io::duplex(4096);
    // Two blank lines before the real content
    w.write_all(b"\n\n{\"y\":42}\n").await.unwrap();
    drop(w);

    let mut reader = LineReader::new(r);
    let val: serde_json::Value =
        timeout(TIMEOUT, reader.next()).await.unwrap().unwrap().unwrap();
    assert_eq!(val["y"], 42);
}

// ---------------------------------------------------------------------------
// Transport layer — LineWriter
// ---------------------------------------------------------------------------

#[tokio::test]
async fn transport_line_writer_writes_jsonl() {
    let (w, r) = tokio::io::duplex(4096);
    let writer = LineWriter::new(w);

    timeout(TIMEOUT, writer.write(&serde_json::json!({"ok": true})))
        .await
        .unwrap()
        .unwrap();
    drop(writer);

    let mut buf = String::new();
    let mut br = BufReader::new(r);
    br.read_line(&mut buf).await.unwrap();

    assert!(buf.ends_with('\n'), "writer must append a newline");
    let val: serde_json::Value = serde_json::from_str(buf.trim()).unwrap();
    assert_eq!(val["ok"], true);
}

#[tokio::test]
async fn transport_line_writer_clone_both_write() {
    let (w, r) = tokio::io::duplex(4096);
    let writer = LineWriter::new(w);
    let writer2 = writer.clone();

    // Both clones write through the same underlying channel
    timeout(TIMEOUT, writer.write(&serde_json::json!({"seq": 1})))
        .await
        .unwrap()
        .unwrap();
    timeout(TIMEOUT, writer2.write(&serde_json::json!({"seq": 2})))
        .await
        .unwrap()
        .unwrap();
    drop(writer);
    drop(writer2);

    let mut br = BufReader::new(r);
    let mut buf = String::new();

    br.read_line(&mut buf).await.unwrap();
    let v1: serde_json::Value = serde_json::from_str(buf.trim()).unwrap();
    buf.clear();
    br.read_line(&mut buf).await.unwrap();
    let v2: serde_json::Value = serde_json::from_str(buf.trim()).unwrap();

    let seqs: std::collections::HashSet<i64> = [
        v1["seq"].as_i64().unwrap(),
        v2["seq"].as_i64().unwrap(),
    ]
    .into_iter()
    .collect();
    assert_eq!(seqs, [1i64, 2].into_iter().collect::<std::collections::HashSet<_>>());
}

// ---------------------------------------------------------------------------
// Full RPC server round-trips
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rpc_session_attach_creates_new_session() {
    let (mut client_w, server_r) = tokio::io::duplex(8192);
    let (server_w, client_r) = tokio::io::duplex(8192);

    send_req(
        &mut client_w,
        &RpcRequest { id: "req-1".into(), method: RpcMethod::SessionAttach { session_id: None } },
    )
    .await;
    drop(client_w);

    let server = RpcServer::new(server_r, server_w);
    server.run().await.unwrap();

    let mut br = BufReader::new(client_r);
    let resp =
        timeout(TIMEOUT, read_resp_for(&mut br, "req-1")).await.unwrap();

    match &resp {
        RpcResponse::Success { request_id, data } => {
            assert_eq!(request_id, "req-1");
            let info: SessionInfo = serde_json::from_value(data.clone()).unwrap();
            assert!(!info.session_id.is_empty(), "session_id must not be empty");
            assert_eq!(info.run_count, 0);
            assert!(
                matches!(info.status.as_str(), "active" | "idle"),
                "unexpected status: {}",
                info.status
            );
        }
        other => panic!("expected Success, got {:?}", other),
    }
}

#[tokio::test]
async fn rpc_session_attach_existing_session() {
    let (mut client_w, server_r) = tokio::io::duplex(8192);
    let (server_w, client_r) = tokio::io::duplex(8192);

    let server = RpcServer::new(server_r, server_w);
    let mut br = BufReader::new(client_r);
    tokio::spawn(server.run());

    // Create a new session
    send_req(
        &mut client_w,
        &RpcRequest { id: "req-1".into(), method: RpcMethod::SessionAttach { session_id: None } },
    )
    .await;
    let resp1 =
        timeout(TIMEOUT, read_resp_for(&mut br, "req-1")).await.unwrap();
    let session_id_str = session_id_from_success(&resp1);

    // Attach to the same session by ID
    let sid = parse_session_id(&session_id_str);
    send_req(
        &mut client_w,
        &RpcRequest {
            id: "req-2".into(),
            method: RpcMethod::SessionAttach { session_id: Some(sid) },
        },
    )
    .await;
    drop(client_w);

    let resp2 =
        timeout(TIMEOUT, read_resp_for(&mut br, "req-2")).await.unwrap();
    match &resp2 {
        RpcResponse::Success { request_id, data } => {
            assert_eq!(request_id, "req-2");
            let info: SessionInfo = serde_json::from_value(data.clone()).unwrap();
            assert_eq!(info.session_id, session_id_str, "must return the same session");
        }
        other => panic!("expected Success, got {:?}", other),
    }
}

#[tokio::test]
async fn rpc_session_detach_success() {
    let (mut client_w, server_r) = tokio::io::duplex(8192);
    let (server_w, client_r) = tokio::io::duplex(8192);

    let server = RpcServer::new(server_r, server_w);
    let mut br = BufReader::new(client_r);
    tokio::spawn(server.run());

    // Attach
    send_req(
        &mut client_w,
        &RpcRequest { id: "att-1".into(), method: RpcMethod::SessionAttach { session_id: None } },
    )
    .await;
    let resp_att =
        timeout(TIMEOUT, read_resp_for(&mut br, "att-1")).await.unwrap();
    let sid = parse_session_id(&session_id_from_success(&resp_att));

    // Detach
    send_req(
        &mut client_w,
        &RpcRequest {
            id: "det-1".into(),
            method: RpcMethod::SessionDetach { session_id: sid },
        },
    )
    .await;
    drop(client_w);

    let resp_det =
        timeout(TIMEOUT, read_resp_for(&mut br, "det-1")).await.unwrap();
    match &resp_det {
        RpcResponse::Success { request_id, data } => {
            assert_eq!(request_id, "det-1");
            assert_eq!(data["detached"], true);
        }
        other => panic!("expected Success, got {:?}", other),
    }
}

#[tokio::test]
async fn rpc_run_start_returns_ack() {
    let (mut client_w, server_r) = tokio::io::duplex(8192);
    let (server_w, client_r) = tokio::io::duplex(8192);

    send_req(
        &mut client_w,
        &RpcRequest {
            id: "run-1".into(),
            method: RpcMethod::RunStart {
                session_id: None,
                provider: None,
                model: None,
                prompt: "hello".into(),
            },
        },
    )
    .await;
    drop(client_w);

    let server = RpcServer::new(server_r, server_w);
    server.run().await.unwrap();

    let mut br = BufReader::new(client_r);
    let resp =
        timeout(TIMEOUT, read_resp_for(&mut br, "run-1")).await.unwrap();
    assert!(
        matches!(resp, RpcResponse::Ack { .. }),
        "RunStart must be acknowledged, got {:?}",
        resp
    );
}

#[tokio::test]
async fn rpc_auth_status_returns_info() {
    let (mut client_w, server_r) = tokio::io::duplex(8192);
    let (server_w, client_r) = tokio::io::duplex(8192);

    send_req(
        &mut client_w,
        &RpcRequest {
            id: "auth-1".into(),
            method: RpcMethod::AuthStatus { provider: ProviderId::new("openai") },
        },
    )
    .await;
    drop(client_w);

    let server = RpcServer::new(server_r, server_w);
    server.run().await.unwrap();

    let mut br = BufReader::new(client_r);
    let resp =
        timeout(TIMEOUT, read_resp_for(&mut br, "auth-1")).await.unwrap();
    match &resp {
        RpcResponse::Success { request_id, data } => {
            assert_eq!(request_id, "auth-1");
            let info: AuthStatusInfo = serde_json::from_value(data.clone()).unwrap();
            assert_eq!(info.provider_id, "openai");
            assert!(!info.authenticated, "mock auth should be unauthenticated");
        }
        other => panic!("expected Success, got {:?}", other),
    }
}

#[tokio::test]
async fn rpc_capabilities_returns_info() {
    let (mut client_w, server_r) = tokio::io::duplex(8192);
    let (server_w, client_r) = tokio::io::duplex(8192);

    send_req(
        &mut client_w,
        &RpcRequest {
            id: "cap-1".into(),
            method: RpcMethod::Capabilities { provider: ProviderId::new("openai") },
        },
    )
    .await;
    drop(client_w);

    let server = RpcServer::new(server_r, server_w);
    server.run().await.unwrap();

    let mut br = BufReader::new(client_r);
    let resp =
        timeout(TIMEOUT, read_resp_for(&mut br, "cap-1")).await.unwrap();
    match &resp {
        RpcResponse::Success { request_id, data } => {
            assert_eq!(request_id, "cap-1");
            let info: CapabilitiesInfo = serde_json::from_value(data.clone()).unwrap();
            assert_eq!(info.protocol_version, "1.0");
            assert!(info.streaming_supported);
            assert_eq!(info.supported_methods.len(), 7);
            for expected_method in
                ["run_start", "run_cancel", "session_attach", "session_detach", "auth_status", "auth_login", "capabilities"]
            {
                assert!(
                    info.supported_methods.contains(&expected_method.to_string()),
                    "missing method: {expected_method}"
                );
            }
        }
        other => panic!("expected Success, got {:?}", other),
    }
}

#[tokio::test]
async fn rpc_run_cancel_cancels_run() {
    let (mut client_w, server_r) = tokio::io::duplex(16384);
    let (server_w, client_r) = tokio::io::duplex(16384);

    let server = RpcServer::new(server_r, server_w);
    let mut br = BufReader::new(client_r);
    tokio::spawn(server.run());

    // Step 1: create a session
    send_req(
        &mut client_w,
        &RpcRequest { id: "att-1".into(), method: RpcMethod::SessionAttach { session_id: None } },
    )
    .await;
    let att_resp =
        timeout(TIMEOUT, read_resp_for(&mut br, "att-1")).await.unwrap();
    let sid = parse_session_id(&session_id_from_success(&att_resp));

    // Step 2: start a run on that session
    send_req(
        &mut client_w,
        &RpcRequest {
            id: "run-1".into(),
            method: RpcMethod::RunStart {
                session_id: Some(sid.clone()),
                provider: None,
                model: None,
                prompt: "cancel me".into(),
            },
        },
    )
    .await;
    // Wait for Ack to confirm run is registered before cancelling
    let _ = timeout(TIMEOUT, read_resp_for(&mut br, "run-1")).await.unwrap();

    // Step 3: cancel by session (server resolves current run_id)
    send_req(
        &mut client_w,
        &RpcRequest {
            id: "cancel-1".into(),
            method: RpcMethod::RunCancel { session_id: sid, run_id: None },
        },
    )
    .await;
    drop(client_w);

    let cancel_resp =
        timeout(TIMEOUT, read_resp_for(&mut br, "cancel-1")).await.unwrap();
    match &cancel_resp {
        RpcResponse::Success { request_id, data } => {
            assert_eq!(request_id, "cancel-1");
            assert_eq!(data["cancelled"], true, "cancelled flag must be true");
        }
        other => panic!("expected Success, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// Streaming
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rpc_run_start_streams_token_chunks() {
    let (mut client_w, server_r) = tokio::io::duplex(16384);
    let (server_w, client_r) = tokio::io::duplex(16384);

    send_req(
        &mut client_w,
        &RpcRequest {
            id: "stream-1".into(),
            method: RpcMethod::RunStart {
                session_id: None,
                provider: None,
                model: None,
                prompt: "stream test".into(),
            },
        },
    )
    .await;
    drop(client_w);

    // server.run() returns only after EOF on the reader side.
    // The background task holds an Arc reference to the writer, so server_w
    // stays open until the task finishes – at which point client_r gets EOF.
    let server = RpcServer::with_state(
        make_state_with_mock(vec!["token chunk 0", "token chunk 1", "token chunk 2"]),
        server_r,
        server_w,
    );
    timeout(TIMEOUT, server.run()).await.unwrap().unwrap();

    let mut br = BufReader::new(client_r);
    let mut responses = Vec::new();
    let mut buf = String::new();
    loop {
        buf.clear();
        let n = timeout(TIMEOUT, br.read_line(&mut buf)).await.unwrap().unwrap();
        if n == 0 {
            break;
        }
        let trimmed = buf.trim();
        if trimmed.is_empty() {
            continue;
        }
        let resp: RpcResponse = serde_json::from_str(trimmed)
            .unwrap_or_else(|e| panic!("parse error: {e}\nraw: {trimmed:?}"));
        responses.push(resp);
    }

    let ack_count = responses.iter().filter(|r| matches!(r, RpcResponse::Ack { .. })).count();
    let stream_events: Vec<_> = responses
        .iter()
        .filter_map(|r| if let RpcResponse::StreamEvent { event } = r { Some(event) } else { None })
        .collect();

    let token_chunks: Vec<_> =
        stream_events.iter().filter(|e| e.event_type == "token_chunk").collect();
    let completed: Vec<_> =
        stream_events.iter().filter(|e| e.event_type == "run_completed").collect();

    assert_eq!(ack_count, 1, "expected exactly 1 Ack, got: {ack_count}");
    assert_eq!(token_chunks.len(), 3, "expected 3 token_chunk StreamEvents");
    assert_eq!(completed.len(), 1, "expected 1 run_completed StreamEvent");
}

#[tokio::test]
async fn streaming_run_start_seq_numbers_increment() {
    let (mut client_w, server_r) = tokio::io::duplex(16384);
    let (server_w, client_r) = tokio::io::duplex(16384);

    send_req(
        &mut client_w,
        &RpcRequest {
            id: "seq-test".into(),
            method: RpcMethod::RunStart {
                session_id: None,
                provider: None,
                model: None,
                prompt: "seq test".into(),
            },
        },
    )
    .await;
    drop(client_w);

    let server = RpcServer::with_state(
        make_state_with_mock(vec!["hello", " world"]),
        server_r,
        server_w,
    );
    timeout(TIMEOUT, server.run()).await.unwrap().unwrap();

    let mut br = BufReader::new(client_r);
    let mut seq_numbers: Vec<u64> = Vec::new();
    let mut buf = String::new();
    loop {
        buf.clear();
        let n = timeout(TIMEOUT, br.read_line(&mut buf)).await.unwrap().unwrap();
        if n == 0 {
            break;
        }
        let trimmed = buf.trim();
        if trimmed.is_empty() {
            continue;
        }
        let resp: RpcResponse = serde_json::from_str(trimmed).unwrap();
        if let RpcResponse::StreamEvent { event } = resp {
            seq_numbers.push(event.seq);
        }
    }

    assert!(!seq_numbers.is_empty(), "expected at least one StreamEvent");
    for window in seq_numbers.windows(2) {
        assert!(
            window[1] > window[0],
            "seq numbers must be strictly increasing; got {:?}",
            seq_numbers
        );
    }
}

// ---------------------------------------------------------------------------
// Error cases
// ---------------------------------------------------------------------------

#[tokio::test]
async fn error_malformed_json_server_continues() {
    let (mut client_w, server_r) = tokio::io::duplex(8192);
    let (server_w, client_r) = tokio::io::duplex(8192);

    // Malformed JSON first
    client_w.write_all(b"this is not valid json\n").await.unwrap();

    // Then a valid request
    send_req(
        &mut client_w,
        &RpcRequest {
            id: "after-err".into(),
            method: RpcMethod::AuthStatus { provider: ProviderId::new("test") },
        },
    )
    .await;
    drop(client_w);

    let server = RpcServer::new(server_r, server_w);
    server.run().await.unwrap();

    let mut br = BufReader::new(client_r);

    // First response: parse error with code "parse_error"
    let err_resp = timeout(TIMEOUT, read_resp(&mut br)).await.unwrap();
    match &err_resp {
        RpcResponse::Error { code, .. } => {
            assert_eq!(code, "parse_error", "expected parse_error code, got {code:?}");
        }
        other => panic!("expected Error response, got {:?}", other),
    }

    // Second response: valid request succeeded → server continued after the error
    let ok_resp =
        timeout(TIMEOUT, read_resp_for(&mut br, "after-err")).await.unwrap();
    assert!(
        matches!(ok_resp, RpcResponse::Success { .. }),
        "server must continue processing after parse error; got {:?}",
        ok_resp
    );
}

#[tokio::test]
async fn error_session_detach_unknown_session() {
    let (mut client_w, server_r) = tokio::io::duplex(8192);
    let (server_w, client_r) = tokio::io::duplex(8192);

    let unknown_sid = agent_core::types::SessionId::new();
    send_req(
        &mut client_w,
        &RpcRequest {
            id: "det-err".into(),
            method: RpcMethod::SessionDetach { session_id: unknown_sid },
        },
    )
    .await;
    drop(client_w);

    let server = RpcServer::new(server_r, server_w);
    server.run().await.unwrap();

    let mut br = BufReader::new(client_r);
    let resp =
        timeout(TIMEOUT, read_resp_for(&mut br, "det-err")).await.unwrap();
    match &resp {
        RpcResponse::Error { request_id, code, .. } => {
            assert_eq!(request_id, "det-err");
            assert_eq!(code, "session_not_found");
        }
        other => panic!("expected Error(session_not_found), got {:?}", other),
    }
}

#[tokio::test]
async fn error_run_cancel_no_active_run() {
    let (mut client_w, server_r) = tokio::io::duplex(8192);
    let (server_w, client_r) = tokio::io::duplex(8192);

    let server = RpcServer::new(server_r, server_w);
    let mut br = BufReader::new(client_r);
    tokio::spawn(server.run());

    // Create a session but do NOT start any run
    send_req(
        &mut client_w,
        &RpcRequest { id: "att-1".into(), method: RpcMethod::SessionAttach { session_id: None } },
    )
    .await;
    let att_resp =
        timeout(TIMEOUT, read_resp_for(&mut br, "att-1")).await.unwrap();
    let sid = parse_session_id(&session_id_from_success(&att_resp));

    // Attempt to cancel with no active run
    send_req(
        &mut client_w,
        &RpcRequest {
            id: "cancel-err".into(),
            method: RpcMethod::RunCancel { session_id: sid, run_id: None },
        },
    )
    .await;
    drop(client_w);

    let resp =
        timeout(TIMEOUT, read_resp_for(&mut br, "cancel-err")).await.unwrap();
    match &resp {
        RpcResponse::Error { request_id, code, .. } => {
            assert_eq!(request_id, "cancel-err");
            assert_eq!(code, "run_not_found");
        }
        other => panic!("expected Error(run_not_found), got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// Server shutdown
// ---------------------------------------------------------------------------

#[tokio::test]
async fn server_exits_cleanly_on_eof() {
    let (client_w, server_r) = tokio::io::duplex(8192);
    let (server_w, _client_r) = tokio::io::duplex(8192);

    // Close the write end immediately so the server sees EOF right away
    drop(client_w);

    let server = RpcServer::new(server_r, server_w);
    let result = timeout(TIMEOUT, server.run()).await;
    assert!(result.is_ok(), "server must exit within {TIMEOUT:?} on EOF");
    assert!(result.unwrap().is_ok(), "server.run() must return Ok(())");
}

// ---------------------------------------------------------------------------
// Phase 13 security: hostile RPC input tests
// ---------------------------------------------------------------------------

/// Sending a JSON object with an unknown method name must return a parse error,
/// not panic or hang.
#[tokio::test]
async fn rpc_unknown_method_returns_error() {
    let (mut client_w, server_r) = tokio::io::duplex(8192);
    let (server_w, client_r) = tokio::io::duplex(8192);

    // Valid JSON structure but unrecognised method name
    client_w
        .write_all(b"{\"id\":\"unk\",\"method\":{\"name\":\"obliterate_everything\"}}\n")
        .await
        .unwrap();
    drop(client_w);

    let server = RpcServer::new(server_r, server_w);
    server.run().await.unwrap();

    let mut br = BufReader::new(client_r);
    let resp = timeout(TIMEOUT, read_resp(&mut br)).await.unwrap();
    match &resp {
        RpcResponse::Error { code, .. } => {
            assert!(
                code == "parse_error" || code == "invalid_request",
                "expected parse_error or invalid_request for unknown method, got: {code}"
            );
        }
        other => panic!("expected Error response for unknown method, got: {other:?}"),
    }
}

/// Multiple consecutive malformed JSON lines must not cause a panic or hang.
/// The server should return errors for each malformed line and exit cleanly on EOF.
#[tokio::test]
async fn rpc_malformed_json_does_not_panic() {
    let (mut client_w, server_r) = tokio::io::duplex(8192);
    let (server_w, client_r) = tokio::io::duplex(8192);

    // Send several lines of garbage JSON — no valid request follows
    client_w.write_all(b"}{totally invalid\n").await.unwrap();
    client_w.write_all(b"<html>not json</html>\n").await.unwrap();
    client_w.write_all(b"[1, 2, 3]\n").await.unwrap();
    drop(client_w);

    let server = RpcServer::new(server_r, server_w);
    // Must complete within the timeout without panicking
    let result = timeout(TIMEOUT, server.run()).await;
    assert!(result.is_ok(), "server must not hang on all-malformed input");
    // server.run() returns Ok(()) on clean exit
    assert!(result.unwrap().is_ok(), "server must exit cleanly on EOF after malformed input");

    // Verify the server emitted parse_error responses (one per bad line)
    let mut br = BufReader::new(client_r);
    let first = timeout(TIMEOUT, read_resp(&mut br)).await.unwrap();
    assert!(
        matches!(first, RpcResponse::Error { .. }),
        "first response to malformed JSON must be an error, got: {first:?}"
    );
}
