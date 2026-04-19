use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for an agent run.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunId(pub Uuid);

impl RunId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for RunId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for RunId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a conversation session.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub Uuid);

impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Identifies a model provider (e.g. "openai", "github-copilot", "llamacpp").
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProviderId(pub String);

impl ProviderId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for ProviderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Identifies a specific model within a provider (e.g. "gpt-4o", "llama-3-70b").
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelId(pub String);

impl ModelId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for ModelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A tool invocation requested by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique ID for this invocation, assigned by the model.
    pub id: String,
    /// The registered tool name.
    pub name: String,
    /// JSON-encoded arguments as provided by the model.
    pub arguments: serde_json::Value,
}

/// The outcome of executing a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Must match the `ToolCall::id` this result corresponds to.
    pub call_id: String,
    /// Whether the tool execution succeeded.
    pub success: bool,
    /// The tool's output, or an error description on failure.
    pub output: serde_json::Value,
}

/// Authentication state for a provider session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum AuthState {
    /// No credentials present.
    Unauthenticated,
    /// Authentication is in progress (e.g. device flow pending).
    Pending {
        flow: AuthFlow,
        /// ISO-8601 expiry of the pending auth challenge.
        expires_at: DateTime<Utc>,
    },
    /// Valid credentials are available.
    Authenticated {
        provider: ProviderId,
        /// ISO-8601 expiry of the current token; None if token does not expire.
        expires_at: Option<DateTime<Utc>>,
    },
    /// Credentials expired and a refresh is needed.
    Expired { provider: ProviderId },
    /// Authentication failed and cannot be retried without user action.
    Failed { reason: String },
}

/// The authentication flow type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthFlow {
    OAuthBrowser,
    DeviceCode,
    ApiKey,
}

/// Top-level agent runtime event.
///
/// All events are append-only; the event log is the source of truth for runtime state.
/// Events are serialized as JSONL for streaming and storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    // --- Session lifecycle ---
    SessionCreated {
        session_id: SessionId,
        timestamp: DateTime<Utc>,
    },
    SessionResumed {
        session_id: SessionId,
        timestamp: DateTime<Utc>,
    },
    SessionEnded {
        session_id: SessionId,
        timestamp: DateTime<Utc>,
    },

    // --- Run lifecycle ---
    RunCreated {
        run_id: RunId,
        session_id: SessionId,
        timestamp: DateTime<Utc>,
    },
    RunQueued {
        run_id: RunId,
        timestamp: DateTime<Utc>,
    },
    RunStarted {
        run_id: RunId,
        session_id: SessionId,
        provider: ProviderId,
        model: ModelId,
        timestamp: DateTime<Utc>,
    },
    RunCompleted {
        run_id: RunId,
        timestamp: DateTime<Utc>,
    },
    RunCancelled {
        run_id: RunId,
        timestamp: DateTime<Utc>,
    },
    RunFailed {
        run_id: RunId,
        reason: String,
        timestamp: DateTime<Utc>,
    },
    InterruptRequested {
        run_id: RunId,
        /// Human-readable reason for the interrupt.
        reason: String,
        timestamp: DateTime<Utc>,
    },
    CancellationRequested {
        run_id: RunId,
        timestamp: DateTime<Utc>,
    },

    // --- Prompt assembly ---
    PromptAssembled {
        run_id: RunId,
        /// Number of sections included in the assembled prompt.
        section_count: usize,
        /// Estimated total token count (placeholder; real accounting in Phase 6).
        estimated_tokens: u32,
        timestamp: DateTime<Utc>,
    },

    // --- Model output ---
    TokenChunk {
        run_id: RunId,
        delta: String,
        timestamp: DateTime<Utc>,
    },
    ToolCallRequested {
        run_id: RunId,
        call: ToolCall,
        timestamp: DateTime<Utc>,
    },
    ToolResultSubmitted {
        run_id: RunId,
        result: ToolResult,
        timestamp: DateTime<Utc>,
    },

    // --- Tool execution ---
    ToolExecutionStarted {
        run_id: RunId,
        call_id: String,
        tool_name: String,
        timestamp: DateTime<Utc>,
    },
    ToolStdout {
        run_id: RunId,
        call_id: String,
        line: String,
        timestamp: DateTime<Utc>,
    },
    ToolStderr {
        run_id: RunId,
        call_id: String,
        line: String,
        timestamp: DateTime<Utc>,
    },
    ToolExecutionCompleted {
        run_id: RunId,
        call_id: String,
        exit_code: Option<i32>,
        timestamp: DateTime<Utc>,
    },
    ToolExecutionFailed {
        run_id: RunId,
        call_id: String,
        reason: String,
        timestamp: DateTime<Utc>,
    },
    ToolExecutionCancelled {
        run_id: RunId,
        call_id: String,
        timestamp: DateTime<Utc>,
    },

    // --- Auth ---
    AuthStateChanged {
        provider: ProviderId,
        state: AuthState,
        timestamp: DateTime<Utc>,
    },
    /// A login attempt was initiated for a provider.
    AuthLoginStarted {
        provider: ProviderId,
        flow: AuthFlow,
        timestamp: DateTime<Utc>,
    },
    /// Login completed successfully and a token was obtained.
    AuthLoginCompleted {
        provider: ProviderId,
        flow: AuthFlow,
        timestamp: DateTime<Utc>,
    },
    /// Login attempt failed.
    AuthLoginFailed {
        provider: ProviderId,
        flow: AuthFlow,
        reason: String,
        timestamp: DateTime<Utc>,
    },
    /// Device authorization flow was started; user must visit a URL.
    DeviceFlowInitiated {
        provider: ProviderId,
        verification_uri: String,
        user_code: String,
        expires_in_secs: u64,
        timestamp: DateTime<Utc>,
    },
    /// Device code was issued and polling has begun.
    DeviceCodeIssued {
        provider: ProviderId,
        interval_secs: u64,
        timestamp: DateTime<Utc>,
    },
    /// A token was successfully stored to persistent storage.
    TokenStored {
        provider: ProviderId,
        timestamp: DateTime<Utc>,
    },
    /// A token was successfully refreshed.
    TokenRefreshed {
        provider: ProviderId,
        timestamp: DateTime<Utc>,
    },
    /// Token refresh failed.
    TokenRefreshFailed {
        provider: ProviderId,
        reason: String,
        timestamp: DateTime<Utc>,
    },
    /// Auth state was loaded from persistent storage on startup.
    AuthStateLoaded {
        provider: ProviderId,
        timestamp: DateTime<Utc>,
    },
    /// Auth state was cleared for a provider.
    AuthStateCleared {
        provider: ProviderId,
        timestamp: DateTime<Utc>,
    },

    // --- Tool lifecycle (fine-grained) ---
    /// Tool execution has started.
    ToolStarted {
        run_id: RunId,
        call_id: String,
        tool_name: String,
        timestamp: DateTime<Utc>,
    },
    /// Tool execution completed successfully.
    ToolCompleted {
        run_id: RunId,
        call_id: String,
        tool_name: String,
        /// Exit code for subprocess tools; None for pure-Rust tools.
        exit_code: Option<i32>,
        timestamp: DateTime<Utc>,
    },
    /// Tool execution was cancelled before completion.
    ToolCancelled {
        run_id: RunId,
        call_id: String,
        tool_name: String,
        timestamp: DateTime<Utc>,
    },
    /// Tool execution failed.
    ToolFailed {
        run_id: RunId,
        call_id: String,
        tool_name: String,
        reason: String,
        timestamp: DateTime<Utc>,
    },

    // --- Context ---
    ContextBuilt {
        run_id: RunId,
        token_count: u32,
        file_count: u32,
        timestamp: DateTime<Utc>,
    },
    ContextCompacted {
        run_id: RunId,
        tokens_before: u32,
        tokens_after: u32,
        timestamp: DateTime<Utc>,
    },

    // --- Security audit ---

    /// A tool execution was blocked by approval policy.
    ApprovalDenied {
        run_id: RunId,
        tool_name: String,
        /// "safe" | "low" | "medium" | "high" | "critical"
        sensitivity: String,
        reason: String,
        timestamp: DateTime<Utc>,
    },

    /// A tool execution was approved (for High/Critical tools).
    ApprovalGranted {
        run_id: RunId,
        tool_name: String,
        sensitivity: String,
        timestamp: DateTime<Utc>,
    },

    /// A shell command was denied by command policy.
    CommandDenied {
        run_id: RunId,
        /// The command is redacted to first 100 chars to avoid logging secrets.
        command_preview: String,
        reason: String,
        timestamp: DateTime<Utc>,
    },

    /// A file path was denied by path safety policy (traversal attempt).
    PathDenied {
        run_id: RunId,
        path: String,
        reason: String,
        timestamp: DateTime<Utc>,
    },

    /// A file overwrite was blocked by overwrite policy.
    OverwriteBlocked {
        run_id: RunId,
        path: String,
        reason: String,
        timestamp: DateTime<Utc>,
    },

    /// A policy rule denied a tool, file, or provider request.
    PolicyDenied {
        /// "tool" | "file" | "provider" | "auth"
        domain: String,
        subject: String,
        rule: Option<String>,
        reason: String,
        timestamp: DateTime<Utc>,
    },
}
