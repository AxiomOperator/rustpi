use std::collections::VecDeque;
use chrono::{DateTime, Utc};
use crate::theme::Theme;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PaneId {
    Conversation,
    Tools,
    Context,
    Sessions,
    Auth,
    Logs,
    DataSources,
}

pub struct AppState {
    pub messages: Vec<ChatMessage>,
    pub streaming_chunk: String,
    pub is_thinking: bool,
    pub tool_events: VecDeque<ToolActivity>,
    pub context_info: Option<ContextInfo>,
    pub sessions: Vec<SessionSummary>,
    pub active_session_id: Option<String>,
    pub session_list_cursor: usize,
    pub providers: Vec<ProviderStatus>,
    pub log_entries: VecDeque<LogEntry>,
    pub data_sources: VecDeque<DataSourceActivity>,
    pub focused_pane: PaneId,
    pub pending_approval: Option<ApprovalRequest>,
    pub active_run_id: Option<String>,
    pub status_message: Option<String>,
    pub theme: Theme,
    pub show_theme_selector: bool,
    pub theme_selector_cursor: usize,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            streaming_chunk: String::new(),
            is_thinking: false,
            tool_events: VecDeque::new(),
            context_info: None,
            sessions: Vec::new(),
            active_session_id: None,
            session_list_cursor: 0,
            providers: Vec::new(),
            log_entries: VecDeque::new(),
            data_sources: VecDeque::new(),
            focused_pane: PaneId::Conversation,
            pending_approval: None,
            active_run_id: None,
            status_message: None,
            theme: crate::theme::matrix(),
            show_theme_selector: false,
            theme_selector_cursor: 0,
        }
    }

    /// Initialize theme from a config name string.
    pub fn with_theme_name(mut self, name: &str) -> Self {
        self.theme = crate::theme::by_name(name);
        self.theme_selector_cursor = crate::theme::all_themes()
            .iter()
            .position(|t| t.name == self.theme.name)
            .unwrap_or(0);
        self
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
pub struct DataSourceActivity {
    pub source: String,
    pub detail: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ContextInfo {
    pub file_count: u32,
    pub token_count: u32,
}
