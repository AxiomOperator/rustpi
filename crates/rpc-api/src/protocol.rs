//! RPC protocol message types.

use agent_core::types::{AgentEvent, ModelId, ProviderId, SessionId};
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
    /// Cancel the current run.
    RunCancel { session_id: SessionId },
    /// Attach to an existing session and receive its event stream.
    SessionAttach { session_id: SessionId },
    /// Detach from a session.
    SessionDetach { session_id: SessionId },
    /// Query auth state for a provider.
    AuthStatus { provider: ProviderId },
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
}
