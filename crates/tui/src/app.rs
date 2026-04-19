use std::collections::HashMap;
use std::io::Stdout;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::broadcast;
use tokio::time::interval;

use agent_core::types::AgentEvent;
use config_core::model::Config;
use rpc_api::server::ServerState;

use crate::input::{map_key, KeyAction};
use crate::layout::compute_layout;
use crate::panes;
use crate::state::{AppState, ChatMessage, ContextInfo, LogEntry, MessageRole, PaneId, ProviderStatus, SessionSummary, ToolActivity, ToolStatus};

pub struct App {
    pub state: AppState,
    pub server_state: Arc<ServerState>,
    pub config: Config,
    pub input_buffer: String,
    pub scroll_offsets: HashMap<PaneId, usize>,
}

impl App {
    pub fn new(config: Config, server_state: Arc<ServerState>) -> Self {
        let providers: Vec<ProviderStatus> = config.providers.iter().map(|p| {
            ProviderStatus {
                id: p.id.to_string(),
                kind: format!("{:?}", p.kind),
                auth_state: "unauthenticated".to_string(),
            }
        }).collect();

        let mut state = AppState::new();
        state.providers = providers;

        Self {
            state,
            server_state,
            config,
            input_buffer: String::new(),
            scroll_offsets: HashMap::new(),
        }
    }

    pub async fn run(mut self) -> Result<()> {
        let original_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic_info| {
            let _ = disable_raw_mode();
            let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
            original_hook(panic_info);
        }));

        enable_raw_mode()?;
        let mut stdout = std::io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let result = self.event_loop(&mut terminal).await;

        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;

        result
    }

    async fn event_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
        let mut tick = interval(Duration::from_millis(250));
        let mut event_stream = EventStream::new();
        let mut agent_rx: broadcast::Receiver<AgentEvent> = self.server_state.event_bus.subscribe();

        for event in self.server_state.event_bus.all_events() {
            apply_agent_event(&mut self.state, event);
        }

        loop {
            terminal.draw(|frame| {
                let area = frame.area();
                let rects = compute_layout(area);

                panes::conversation::render(frame, &self.state, rects.conversation, &self.input_buffer);
                panes::tools::render(frame, &self.state, rects.tools);
                panes::session::render(frame, &self.state, rects.sessions);
                panes::context::render(frame, &self.state, rects.context);
                panes::auth::render(frame, &self.state, rects.auth);
                panes::logs::render(frame, &self.state, rects.logs);

                render_status_bar(frame, &self.state, rects.status_bar);
            })?;

            tokio::select! {
                _ = tick.tick() => {}
                maybe_event = event_stream.next() => {
                    match maybe_event {
                        Some(Ok(Event::Key(key))) => {
                            if key.kind == KeyEventKind::Press {
                                let action = map_key(key);
                                if self.handle_action(action) {
                                    break;
                                }
                            }
                        }
                        Some(Ok(Event::Resize(w, h))) => {
                            let _ = (w, h);
                        }
                        Some(Err(_)) | None => break,
                        _ => {}
                    }
                }
                result = agent_rx.recv() => {
                    match result {
                        Ok(event) => apply_agent_event(&mut self.state, event),
                        Err(broadcast::error::RecvError::Lagged(_)) => {}
                        Err(broadcast::error::RecvError::Closed) => {}
                    }
                }
            }
        }

        Ok(())
    }

    /// Returns true if the app should quit.
    fn handle_action(&mut self, action: KeyAction) -> bool {
        match action {
            KeyAction::Quit => return true,
            KeyAction::FocusConversation => self.state.focused_pane = PaneId::Conversation,
            KeyAction::FocusTools => self.state.focused_pane = PaneId::Tools,
            KeyAction::FocusContext => self.state.focused_pane = PaneId::Context,
            KeyAction::FocusSessions => self.state.focused_pane = PaneId::Sessions,
            KeyAction::FocusAuth => self.state.focused_pane = PaneId::Auth,
            KeyAction::FocusLogs => self.state.focused_pane = PaneId::Logs,
            KeyAction::TypeChar(c) => self.input_buffer.push(c),
            KeyAction::Backspace => { self.input_buffer.pop(); }
            KeyAction::SubmitInput => {
                if !self.input_buffer.is_empty() {
                    let content = self.input_buffer.clone();
                    self.input_buffer.clear();
                    self.state.messages.push(ChatMessage {
                        role: MessageRole::User,
                        content,
                        timestamp: chrono::Utc::now(),
                    });
                    self.state.status_message = Some("(prompt submitted — no model connected)".to_string());
                }
            }
            KeyAction::ScrollUp => {
                let offset = self.scroll_offsets.entry(self.state.focused_pane.clone()).or_insert(0);
                *offset = offset.saturating_add(1);
            }
            KeyAction::ScrollDown => {
                let offset = self.scroll_offsets.entry(self.state.focused_pane.clone()).or_insert(0);
                if *offset > 0 { *offset -= 1; }
            }
            KeyAction::Interrupt => {
                if let Some(run_id_str) = &self.state.active_run_id.clone() {
                    self.state.log_entries.push_back(LogEntry {
                        level: "info".to_string(),
                        message: format!("Interrupt requested for run {}", run_id_str),
                        timestamp: chrono::Utc::now(),
                    });
                    self.state.status_message = Some("Interrupt requested".to_string());
                }
            }
            KeyAction::ApproveAction => {
                if self.state.pending_approval.take().is_some() {
                    self.state.status_message = Some("Action approved".to_string());
                }
            }
            KeyAction::DenyAction => {
                if self.state.pending_approval.take().is_some() {
                    self.state.status_message = Some("Action denied".to_string());
                }
            }
            KeyAction::Help => {
                self.state.status_message = Some(
                    "Keys: 1-6 focus panes | q quit | Ctrl+C quit | j/k scroll | Ctrl+I interrupt | y/n approve".to_string()
                );
            }
            KeyAction::PageUp => {
                let offset = self.scroll_offsets.entry(self.state.focused_pane.clone()).or_insert(0);
                *offset = offset.saturating_add(10);
            }
            KeyAction::PageDown => {
                let offset = self.scroll_offsets.entry(self.state.focused_pane.clone()).or_insert(0);
                if *offset >= 10 { *offset -= 10; } else { *offset = 0; }
            }
            KeyAction::None => {}
        }
        false
    }
}

pub fn apply_agent_event(state: &mut AppState, event: AgentEvent) {
    match event {
        AgentEvent::TokenChunk { delta, .. } => {
            state.streaming_chunk.push_str(&delta);
        }
        AgentEvent::RunCompleted { run_id, .. } => {
            if !state.streaming_chunk.is_empty() {
                let content = state.streaming_chunk.clone();
                state.streaming_chunk.clear();
                state.messages.push(ChatMessage {
                    role: MessageRole::Assistant,
                    content,
                    timestamp: chrono::Utc::now(),
                });
            }
            state.active_run_id = None;
            state.log_entries.push_back(LogEntry {
                level: "info".to_string(),
                message: format!("Run {} completed", run_id),
                timestamp: chrono::Utc::now(),
            });
            trim_log(state);
        }
        AgentEvent::RunFailed { run_id, reason, .. } => {
            if !state.streaming_chunk.is_empty() {
                let content = state.streaming_chunk.clone();
                state.streaming_chunk.clear();
                state.messages.push(ChatMessage {
                    role: MessageRole::Assistant,
                    content,
                    timestamp: chrono::Utc::now(),
                });
            }
            state.active_run_id = None;
            state.log_entries.push_back(LogEntry {
                level: "error".to_string(),
                message: format!("Run {} failed: {}", run_id, reason),
                timestamp: chrono::Utc::now(),
            });
            trim_log(state);
        }
        AgentEvent::RunStarted { run_id, .. } => {
            state.active_run_id = Some(run_id.to_string());
            state.log_entries.push_back(LogEntry {
                level: "info".to_string(),
                message: format!("Run {} started", run_id),
                timestamp: chrono::Utc::now(),
            });
            trim_log(state);
        }
        AgentEvent::ToolExecutionStarted { call_id, tool_name, .. } => {
            while state.tool_events.len() >= 200 {
                state.tool_events.pop_front();
            }
            state.tool_events.push_back(ToolActivity {
                call_id,
                tool_name,
                status: ToolStatus::Started,
                output: None,
                timestamp: chrono::Utc::now(),
            });
        }
        AgentEvent::ToolStdout { call_id, line, .. } => {
            if let Some(entry) = state.tool_events.iter_mut().rev().find(|t| t.call_id == call_id) {
                entry.status = ToolStatus::Stdout(line.clone());
                entry.output = Some(line);
            } else {
                while state.tool_events.len() >= 200 {
                    state.tool_events.pop_front();
                }
                state.tool_events.push_back(ToolActivity {
                    call_id,
                    tool_name: "(unknown)".to_string(),
                    status: ToolStatus::Stdout(line.clone()),
                    output: Some(line),
                    timestamp: chrono::Utc::now(),
                });
            }
        }
        AgentEvent::ToolStderr { call_id, line, .. } => {
            if let Some(entry) = state.tool_events.iter_mut().rev().find(|t| t.call_id == call_id) {
                entry.status = ToolStatus::Stderr(line.clone());
                entry.output = Some(line);
            } else {
                while state.tool_events.len() >= 200 {
                    state.tool_events.pop_front();
                }
                state.tool_events.push_back(ToolActivity {
                    call_id,
                    tool_name: "(unknown)".to_string(),
                    status: ToolStatus::Stderr(line.clone()),
                    output: Some(line),
                    timestamp: chrono::Utc::now(),
                });
            }
        }
        AgentEvent::ToolExecutionCompleted { call_id, .. } => {
            if let Some(entry) = state.tool_events.iter_mut().rev().find(|t| t.call_id == call_id) {
                entry.status = ToolStatus::Completed;
            }
        }
        AgentEvent::ToolExecutionFailed { call_id, reason, .. } => {
            if let Some(entry) = state.tool_events.iter_mut().rev().find(|t| t.call_id == call_id) {
                entry.status = ToolStatus::Failed(reason);
            }
        }
        AgentEvent::ToolExecutionCancelled { call_id, .. } => {
            if let Some(entry) = state.tool_events.iter_mut().rev().find(|t| t.call_id == call_id) {
                entry.status = ToolStatus::Cancelled;
            }
        }
        AgentEvent::AuthStateChanged { provider, state: auth_state, .. } => {
            use agent_core::types::AuthState;
            let auth_str = match auth_state {
                AuthState::Authenticated { .. } => "authenticated",
                AuthState::Pending { .. } => "pending",
                AuthState::Expired { .. } => "expired",
                AuthState::Failed { .. } => "failed",
                AuthState::Unauthenticated => "unauthenticated",
            };
            if let Some(p) = state.providers.iter_mut().find(|p| p.id == provider.to_string()) {
                p.auth_state = auth_str.to_string();
            } else {
                state.providers.push(ProviderStatus {
                    id: provider.to_string(),
                    kind: "unknown".to_string(),
                    auth_state: auth_str.to_string(),
                });
            }
        }
        AgentEvent::ContextBuilt { token_count, file_count, .. } => {
            state.context_info = Some(ContextInfo {
                file_count,
                token_count,
            });
        }
        AgentEvent::SessionCreated { session_id, timestamp } => {
            if !state.sessions.iter().any(|s| s.id == session_id.to_string()) {
                state.sessions.push(SessionSummary {
                    id: session_id.to_string(),
                    status: "active".to_string(),
                    run_count: 0,
                    created_at: timestamp.format("%Y-%m-%d %H:%M").to_string(),
                });
            }
            if state.active_session_id.is_none() {
                state.active_session_id = Some(session_id.to_string());
            }
        }
        AgentEvent::ToolCallRequested { run_id, call, .. } => {
            if call.name.contains("write") || call.name.contains("exec") || call.name.contains("shell") || call.name.contains("delete") {
                state.pending_approval = Some(crate::state::ApprovalRequest {
                    run_id: run_id.to_string(),
                    call_id: call.id.clone(),
                    tool_name: call.name.clone(),
                    description: format!("Tool: {} args: {}", call.name, call.arguments),
                });
            }
            state.log_entries.push_back(LogEntry {
                level: "info".to_string(),
                message: format!("Tool call requested: {} ({})", call.name, call.id),
                timestamp: chrono::Utc::now(),
            });
            trim_log(state);
        }
        other => {
            let msg = format!("{:?}", other);
            let msg = if msg.len() > 120 { format!("{}…", &msg[..120]) } else { msg };
            state.log_entries.push_back(LogEntry {
                level: "debug".to_string(),
                message: msg,
                timestamp: chrono::Utc::now(),
            });
            trim_log(state);
        }
    }
}

fn trim_log(state: &mut AppState) {
    while state.log_entries.len() > 500 {
        state.log_entries.pop_front();
    }
}

fn render_status_bar(frame: &mut ratatui::Frame, state: &AppState, area: ratatui::layout::Rect) {
    use ratatui::style::{Color, Style};
    use ratatui::widgets::Paragraph;

    let session = state.active_session_id.as_deref().unwrap_or("none");
    let run_status = if state.active_run_id.is_some() { "active" } else { "idle" };
    let msg = state.status_message.as_deref().unwrap_or("");

    let text = format!(
        " [Session: {}] [Run: {}] {} | Keys: 1-6 panes | q quit | Ctrl+I interrupt | y/n approve | ? help",
        &session[..session.len().min(8)],
        run_status,
        msg,
    );

    let para = Paragraph::new(text)
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));
    frame.render_widget(para, area);
}
