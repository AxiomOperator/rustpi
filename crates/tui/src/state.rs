use std::collections::VecDeque;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PaneId {
    Conversation,
    Tools,
    Context,
    Sessions,
    Auth,
    Logs,
}

pub struct AppState {
    pub messages: Vec<ChatMessage>,
    pub streaming_chunk: String,
    pub tool_events: VecDeque<ToolActivity>,
    pub context_info: Option<ContextInfo>,
    pub sessions: Vec<SessionSummary>,
    pub active_session_id: Option<String>,
    pub session_list_cursor: usize,
    pub providers: Vec<ProviderStatus>,
    pub log_entries: VecDeque<LogEntry>,
    pub focused_pane: PaneId,
    pub pending_approval: Option<ApprovalRequest>,
    pub active_run_id: Option<String>,
    pub status_message: Option<String>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            streaming_chunk: String::new(),
            tool_events: VecDeque::new(),
            context_info: None,
            sessions: Vec::new(),
            active_session_id: None,
            session_list_cursor: 0,
            providers: Vec::new(),
            log_entries: VecDeque::new(),
            focused_pane: PaneId::Conversation,
            pending_approval: None,
            active_run_id: None,
            status_message: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
}

#[derive(Debug, Clone)]
pub struct ToolActivity {
    pub call_id: String,
    pub tool_name: String,
    pub status: ToolStatus,
    pub output: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub enum ToolStatus {
    Started,
    Stdout(String),
    Stderr(String),
    Completed,
    Failed(String),
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct ProviderStatus {
    pub id: String,
    pub kind: String,
    pub auth_state: String,
}

#[derive(Debug, Clone)]
pub struct ApprovalRequest {
    pub run_id: String,
    pub call_id: String,
    pub tool_name: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub level: String,
    pub message: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub id: String,
    pub status: String,
    pub run_count: usize,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct ContextInfo {
    pub file_count: u32,
    pub token_count: u32,
}
