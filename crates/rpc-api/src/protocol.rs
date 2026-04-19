//! RPC protocol message types.

use agent_core::types::{AgentEvent, ModelId, ProviderId, RunId, SessionId};
use serde::{Deserialize, Serialize};

/// An RPC request from the client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRequest {
    /// Client-assigned request ID for correlation.
    pub id: String,
    pub method: RpcMethod,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "name", rename_all = "snake_case")]
pub enum RpcMethod {
    /// Start a new run with a prompt.
    RunStart {
        session_id: Option<SessionId>,
        provider: Option<ProviderId>,
        model: Option<ModelId>,
        prompt: String,
    },
    /// Cancel the current run (optionally specifying a run_id).
    RunCancel {
        session_id: SessionId,
        #[serde(default)]
        run_id: Option<RunId>,
    },
    /// Attach to an existing session or create a new one.
    SessionAttach {
        #[serde(default)]
        session_id: Option<SessionId>,
    },
    /// Detach from a session.
    SessionDetach { session_id: SessionId },
    /// Query auth state for a provider.
    AuthStatus { provider: ProviderId },
    /// Initiate authentication for a provider.
    AuthLogin {
        provider: ProviderId,
        /// Optional client_id override (used for device code flows).
        #[serde(default)]
        client_id: Option<String>,
    },
    /// Request provider capability info.
    Capabilities { provider: ProviderId },
}

/// An RPC response or event from the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RpcResponse {
    /// Acknowledgment of a request.
    Ack { request_id: String },
    /// A runtime event (may be streamed).
    Event { event: AgentEvent },
    /// Terminal error for a request.
    Error { request_id: String, code: String, message: String },
    /// Successful response with structured data.
    Success { request_id: String, data: serde_json::Value },
    /// Streaming event not tied to a request.
    StreamEvent { event: RpcEvent },
}

/// Normalized streaming event sent to hosts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcEvent {
    /// Sequential number for ordering (monotonically increasing per server instance).
    pub seq: u64,
    /// ISO8601 timestamp.
    pub timestamp: String,
    /// Event category for routing/filtering.
    pub category: EventCategory,
    /// Session this event belongs to, if applicable.
    pub session_id: Option<String>,
    /// Run this event belongs to, if applicable.
    pub run_id: Option<String>,
    /// Event type tag (e.g. "run_started", "token_chunk", "tool_stdout").
    pub event_type: String,
    /// Normalized event payload — safe for external consumption.
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventCategory {
    Session,
    Run,
    Tool,
    Auth,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub status: String,
    pub created_at: String,
    pub run_count: usize,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunInfo {
    pub run_id: String,
    pub session_id: String,
    pub status: String,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthStatusInfo {
    pub provider_id: String,
    pub authenticated: bool,
    pub token_expires_at: Option<String>,
    pub flow: Option<String>,
}

/// Response type for AuthLogin — describes the outcome of a login attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthLoginInfo {
    pub provider_id: String,
    pub authenticated: bool,
    /// For device code flows: the code the user must enter at `verification_uri`.
    pub user_code: Option<String>,
    /// For device code flows: the URL the user must visit.
    pub verification_uri: Option<String>,
    /// Human-readable message describing next steps.
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilitiesInfo {
    pub protocol_version: String,
    pub supported_methods: Vec<String>,
    pub streaming_supported: bool,
    pub tool_passthrough: bool,
    pub max_concurrent_runs: usize,
}

/// Structured error code for RPC errors.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RpcErrorCode {
    ParseError,
    InvalidRequest,
    UnknownMethod,
    SessionNotFound,
    RunNotFound,
    InvalidRunState,
    AuthUnavailable,
    CapabilityUnavailable,
    InternalError,
}

impl std::fmt::Display for RpcErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| format!("{:?}", self));
        write!(f, "{}", s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpc_request_roundtrip() {
        let req = RpcRequest {
            id: "test-1".into(),
            method: RpcMethod::Capabilities {
                provider: ProviderId::new("openai"),
            },
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: RpcRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, req.id);
    }

    #[test]
    fn rpc_response_success_roundtrip() {
        let resp = RpcResponse::Success {
            request_id: "req-1".into(),
            data: serde_json::json!({ "ok": true }),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let decoded: RpcResponse = serde_json::from_str(&json).unwrap();
        assert!(matches!(decoded, RpcResponse::Success { .. }));
    }

    #[test]
    fn rpc_event_roundtrip() {
        let ev = RpcEvent {
            seq: 42,
            timestamp: "2024-01-01T00:00:00Z".into(),
            category: EventCategory::Run,
            session_id: Some("sess-1".into()),
            run_id: Some("run-1".into()),
            event_type: "token_chunk".into(),
            payload: serde_json::json!({ "delta": "hello" }),
        };
        let json = serde_json::to_string(&ev).unwrap();
        let decoded: RpcEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.seq, 42);
        assert_eq!(decoded.event_type, "token_chunk");
    }

    #[test]
    fn capabilities_info_roundtrip() {
        let info = CapabilitiesInfo {
            protocol_version: "1.0".into(),
            supported_methods: vec!["run_start".into(), "capabilities".into()],
            streaming_supported: true,
            tool_passthrough: false,
            max_concurrent_runs: 4,
        };
        let json = serde_json::to_string(&info).unwrap();
        let decoded: CapabilitiesInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.protocol_version, "1.0");
        assert_eq!(decoded.supported_methods.len(), 2);
    }
}
