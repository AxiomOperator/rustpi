use std::collections::HashMap;
use std::io::Stdout;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste, Event, EventStream, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::time::interval;

use agent_core::types::{AgentEvent, SessionId};
use config_core::model::Config;
use rpc_api::protocol::{RpcMethod, RpcRequest, RpcResponse};
use rpc_api::server::ServerState;
use rpc_api::{LineReader, LineWriter};

use crate::input::{map_key, KeyAction};
use crate::layout::compute_layout;
use crate::panes;
use crate::state::{AppState, ChatMessage, ContextInfo, DataSourceActivity, LogEntry, MessageRole, PaneId, ProviderStatus, SessionSummary, ToolActivity, ToolStatus};

pub struct App {
    pub state: AppState,
    pub server_state: Arc<ServerState>,
    pub config: Config,
    pub input_buffer: String,
    pub scroll_offsets: HashMap<PaneId, usize>,
    pub input_tx: Option<mpsc::Sender<String>>,
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

        let mut state = AppState::new().with_theme_name(&config.tui.theme);
        state.providers = providers;

        // Resolve the active model name from config (user preferred → project default → global default)
        state.model_name = config.user.preferred_model
            .as_ref()
            .or(config.project.default_model.as_ref())
            .or(config.global.default_model.as_ref())
            .map(|m| m.to_string())
            .unwrap_or_else(|| "unknown".to_string());

        Self {
            state,
            server_state,
            config,
            input_buffer: String::new(),
            scroll_offsets: HashMap::new(),
            input_tx: None,
        }
    }

    pub async fn run(mut self) -> Result<()> {
        let original_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic_info| {
            let _ = disable_raw_mode();
            let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
            original_hook(panic_info);
        }));

        // Set up input channel and spawn background run task before entering TUI.
        let (input_tx, input_rx) = mpsc::channel::<String>(16);
        self.input_tx = Some(input_tx);
        let state_for_bg = Arc::clone(&self.server_state);
        tokio::spawn(run_pipeline(state_for_bg, input_rx));

        enable_raw_mode()?;
        let mut stdout = std::io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableBracketedPaste)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let result = self.event_loop(&mut terminal).await;

        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableBracketedPaste)?;
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
                panes::datasources::render(frame, rects.data_sources, &self.state);
                panes::auth::render(frame, &self.state, rects.auth);
                panes::logs::render(frame, &self.state, rects.logs);

                render_status_bar(frame, &self.state, rects.status_bar);

                if self.state.show_theme_selector {
                    render_theme_selector(frame, &self.state);
                }
            })?;

            tokio::select! {
                _ = tick.tick() => {}
                maybe_event = event_stream.next() => {
                    match maybe_event {
                        Some(Ok(Event::Key(key))) => {
                            if key.kind == KeyEventKind::Press {
                                // Theme selector is a modal — handle it before normal dispatch
                                if self.state.show_theme_selector {
                                    if self.handle_theme_selector_key(key) {
                                        break; // quit signal from modal (unlikely but safe)
                                    }
                                } else {
                                    let action = map_key(key);
                                    if self.handle_action(action) {
                                        break;
                                    }
                                }
                            }
                        }
                        Some(Ok(Event::Paste(text))) => {
                            // Bracketed paste: insert entire pasted string at once (no per-char redraws)
                            for c in text.chars() {
                                if c != '\r' && c != '\n' {
                                    self.input_buffer.push(c);
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
            KeyAction::FocusDataSources => self.state.focused_pane = PaneId::DataSources,
            KeyAction::TypeChar(c) => {
                // y/n are contextual: approve/deny only when a tool approval is pending,
                // otherwise type the character normally.
                if c == 'y' && self.state.pending_approval.is_some() {
                    self.state.pending_approval.take();
                    self.state.status_message = Some("Action approved".to_string());
                } else if c == 'n' && self.state.pending_approval.is_some() {
                    self.state.pending_approval.take();
                    self.state.status_message = Some("Action denied".to_string());
                } else {
                    self.input_buffer.push(c);
                }
            }
            KeyAction::Backspace => { self.input_buffer.pop(); }
            KeyAction::SubmitInput => {
                if !self.input_buffer.is_empty() {
                    let content = self.input_buffer.clone();
                    self.input_buffer.clear();
                    self.state.messages.push(ChatMessage {
                        role: MessageRole::User,
                        content: content.clone(),
                        timestamp: chrono::Utc::now(),
                    });
                    if let Some(tx) = &self.input_tx {
                        let _ = tx.try_send(content);
                        self.state.status_message = Some("Processing…".to_string());
                    } else {
                        self.state.status_message = Some("(no model connected)".to_string());
                    }
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
            KeyAction::ApproveAction | KeyAction::DenyAction => {}
            KeyAction::Help => {
                self.state.status_message = Some(
                    "Keys: 1-7 focus panes | q quit | Ctrl+C quit | ↑/↓ scroll | PgUp/PgDn | Ctrl+I interrupt | y/n approve | Ctrl+T theme".to_string()
                );
            }
            KeyAction::OpenThemeSelector => {
                self.state.show_theme_selector = true;
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

    /// Handle keys when the theme selector modal is open. Returns true only if app should quit.
    fn handle_theme_selector_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::{KeyCode, KeyModifiers};
        let themes = crate::theme::all_themes();
        let count = themes.len();
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) | (KeyCode::Char('t'), KeyModifiers::CONTROL) => {
                self.state.show_theme_selector = false;
            }
            (KeyCode::Char('q'), KeyModifiers::NONE) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                self.state.show_theme_selector = false;
                return true; // propagate quit
            }
            (KeyCode::Up, _) | (KeyCode::Char('k'), _) => {
                if self.state.theme_selector_cursor > 0 {
                    self.state.theme_selector_cursor -= 1;
                } else {
                    self.state.theme_selector_cursor = count.saturating_sub(1);
                }
            }
            (KeyCode::Down, _) | (KeyCode::Char('j'), _) => {
                self.state.theme_selector_cursor = (self.state.theme_selector_cursor + 1) % count;
            }
            (KeyCode::Enter, _) => {
                if let Some(chosen) = themes.into_iter().nth(self.state.theme_selector_cursor) {
                    self.state.theme = chosen;
                }
                self.state.show_theme_selector = false;
            }
            _ => {}
        }
        false
    }
}

/// Background task: creates one TUI session then executes runs for each submitted prompt.
/// Events are delivered back to the TUI via the shared `event_bus`.
async fn run_pipeline(state: Arc<ServerState>, mut input_rx: mpsc::Receiver<String>) {
    // Attach / create a session.
    let session_id: Option<SessionId> = {
        let request = RpcRequest {
            id: uuid::Uuid::new_v4().to_string(),
            method: RpcMethod::SessionAttach { session_id: None },
        };
        let (writer_half, reader_half) = tokio::io::duplex(65_536);
        let line_writer = LineWriter::new(writer_half);
        let _ = rpc_api::dispatch::dispatch(&request, &state, &line_writer).await;
        drop(line_writer);
        let mut reader = LineReader::new(reader_half);
        let mut sid = None;
        while let Some(Ok(resp)) = reader.next::<RpcResponse>().await {
            if let RpcResponse::Success { data, .. } = resp {
                if let Ok(info) = serde_json::from_value::<rpc_api::protocol::SessionInfo>(data) {
                    if let Ok(uuid) = uuid::Uuid::parse_str(&info.session_id) {
                        sid = Some(SessionId(uuid));
                    }
                    break;
                }
            }
        }
        sid
    };

    // Process each prompt sequentially.
    while let Some(prompt) = input_rx.recv().await {
        let request = RpcRequest {
            id: uuid::Uuid::new_v4().to_string(),
            method: RpcMethod::RunStart {
                session_id: session_id.clone(),
                provider: None,
                model: None,
                prompt,
            },
        };
        let (writer_half, reader_half) = tokio::io::duplex(1_048_576);
        let line_writer = LineWriter::new(writer_half);
        let _ = rpc_api::dispatch::dispatch(&request, &state, &line_writer).await;
        drop(line_writer);
        // Drain responses; the run events arrive via the broadcast event_bus, which
        // the TUI render loop already subscribes to.
        let mut reader = LineReader::new(reader_half);
        while reader.next::<RpcResponse>().await.is_some() {}
    }
}

pub fn apply_agent_event(state: &mut AppState, event: AgentEvent) {
    match event {
        AgentEvent::TokenChunk { delta, .. } => {
            state.is_thinking = false;
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
            state.is_thinking = false;
            state.status_message = Some("Ready".to_string());
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
            state.is_thinking = false;
            state.status_message = Some(format!("Error: {}", reason));
            state.log_entries.push_back(LogEntry {
                level: "error".to_string(),
                message: format!("Run {} failed: {}", run_id, reason),
                timestamp: chrono::Utc::now(),
            });
            trim_log(state);
        }
        AgentEvent::RunStarted { run_id, .. } => {
            state.active_run_id = Some(run_id.to_string());
            state.is_thinking = true;
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
        AgentEvent::ToolResultSubmitted { result, .. } => {
            // Surface tool results visibly in the conversation so the user doesn't have to ask.
            let summary = if result.success {
                // For memory_note, extract the saved path for a clear confirmation.
                if let Some(saved) = result.output.get("saved").and_then(|v| v.as_str()) {
                    format!("✓ Saved to {}", saved)
                } else {
                    let out = result.output.to_string();
                    format!("✓ Tool result: {}", if out.len() > 120 { format!("{}…", &out[..120]) } else { out })
                }
            } else {
                let err_owned = result.output.get("error").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_else(|| result.output.to_string());
                format!("✗ Tool error: {}", err_owned)
            };
            state.messages.push(ChatMessage {
                role: MessageRole::Tool,
                content: summary.clone(),
                timestamp: chrono::Utc::now(),
            });
            state.log_entries.push_back(LogEntry {
                level: if result.success { "info" } else { "warn" }.to_string(),
                message: summary,
                timestamp: chrono::Utc::now(),
            });
            trim_log(state);
        }
        AgentEvent::DataSourceAccessed { source, detail, timestamp, .. } => {
            while state.data_sources.len() >= 200 {
                state.data_sources.pop_front();
            }
            state.data_sources.push_back(DataSourceActivity {
                source,
                detail,
                timestamp,
            });
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
    use ratatui::style::Style;
    use ratatui::widgets::Paragraph;

    let session = state.active_session_id.as_deref().unwrap_or("none");
    let run_status = if state.active_run_id.is_some() { "active" } else { "idle" };
    let msg = state.status_message.as_deref().unwrap_or("");
    let model = &state.model_name;
    let theme = &state.theme;

    let text = format!(
        " [Model: {}] [Session: {}] [Run: {}] {} | 1-7 panes | q quit | Ctrl+I interrupt | y/n approve | Ctrl+T theme | ? help",
        model,
        &session[..session.len().min(8)],
        run_status,
        msg,
    );

    let para = Paragraph::new(text)
        .style(Style::default().bg(theme.status_bar_bg).fg(theme.status_bar_fg));
    frame.render_widget(para, area);
}

fn render_theme_selector(frame: &mut ratatui::Frame, state: &AppState) {
    use ratatui::layout::{Constraint, Direction, Layout, Rect};
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState};

    let theme = &state.theme;
    let themes = crate::theme::all_themes();

    // Center a popup: 30 wide, themes.len()+4 tall
    let popup_w = 34u16;
    let popup_h = (themes.len() as u16) + 4;
    let area = frame.area();
    let x = area.x + area.width.saturating_sub(popup_w) / 2;
    let y = area.y + area.height.saturating_sub(popup_h) / 2;
    let popup_rect = Rect { x, y, width: popup_w.min(area.width), height: popup_h.min(area.height) };

    frame.render_widget(Clear, popup_rect);

    let block = Block::default()
        .title(" Select Theme (↑/↓ Enter) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_focused));

    let items: Vec<ListItem> = themes.iter().enumerate().map(|(i, t)| {
        if i == state.theme_selector_cursor {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("▶ {}", t.name),
                    Style::default()
                        .fg(theme.border_focused)
                        .add_modifier(Modifier::BOLD),
                ),
            ]))
        } else {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("  {}", t.name),
                    Style::default().fg(theme.text_primary),
                ),
            ]))
        }
    }).collect();

    // Add a hint line at bottom
    let hint = Line::from(Span::styled(
        "  Esc to cancel",
        Style::default().fg(theme.text_dim),
    ));
    let mut all_items = items;
    all_items.push(ListItem::new(Line::from("")));
    all_items.push(ListItem::new(hint));

    let list = List::new(all_items).block(block);
    let mut list_state = ListState::default();
    list_state.select(Some(state.theme_selector_cursor));
    frame.render_stateful_widget(list, popup_rect, &mut list_state);
}
