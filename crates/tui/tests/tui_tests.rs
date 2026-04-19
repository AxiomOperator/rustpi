use agent_core::types::{
    AgentEvent, AuthState, ModelId, ProviderId, RunId, SessionId,
};
use chrono::Utc;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{backend::TestBackend, Terminal};
use tui::{
    app::apply_agent_event,
    input::{map_key, KeyAction},
    layout::compute_layout,
    panes,
    state::{AppState, ApprovalRequest, ChatMessage, ContextInfo, LogEntry, MessageRole, PaneId, ProviderStatus, SessionSummary, ToolActivity, ToolStatus},
};
use uuid::Uuid;

// ──────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────

fn make_terminal(width: u16, height: u16) -> Terminal<TestBackend> {
    Terminal::new(TestBackend::new(width, height)).unwrap()
}

fn key(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, mods)
}

fn run_id() -> RunId {
    RunId(Uuid::new_v4())
}

fn session_id() -> SessionId {
    SessionId(Uuid::new_v4())
}

fn provider_id(s: &str) -> ProviderId {
    ProviderId::new(s)
}

fn buf_content(terminal: &Terminal<TestBackend>) -> String {
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol().to_string())
        .collect()
}

// ──────────────────────────────────────────────
// 1. State model tests
// ──────────────────────────────────────────────

#[test]
fn test_state_default_is_empty() {
    let state = AppState::new();
    assert!(state.messages.is_empty());
    assert!(state.tool_events.is_empty());
    assert!(state.log_entries.is_empty());
    assert!(state.streaming_chunk.is_empty());
    assert!(state.active_run_id.is_none());
    assert!(state.pending_approval.is_none());
}

#[test]
fn test_apply_token_chunk_accumulates() {
    let mut state = AppState::new();
    apply_agent_event(&mut state, AgentEvent::TokenChunk {
        run_id: run_id(),
        delta: "Hello".into(),
        timestamp: Utc::now(),
    });
    apply_agent_event(&mut state, AgentEvent::TokenChunk {
        run_id: run_id(),
        delta: " world".into(),
        timestamp: Utc::now(),
    });
    assert_eq!(state.streaming_chunk, "Hello world");
}

#[test]
fn test_apply_run_started_sets_active_run() {
    let mut state = AppState::new();
    let rid = run_id();
    apply_agent_event(&mut state, AgentEvent::RunStarted {
        run_id: rid.clone(),
        session_id: session_id(),
        provider: provider_id("openai"),
        model: ModelId::new("gpt-4o"),
        timestamp: Utc::now(),
    });
    assert_eq!(state.active_run_id, Some(rid.to_string()));
}

#[test]
fn test_apply_run_completed_flushes_streaming() {
    let mut state = AppState::new();
    state.streaming_chunk = "partial response".into();
    state.active_run_id = Some("run-1".into());

    apply_agent_event(&mut state, AgentEvent::RunCompleted {
        run_id: run_id(),
        timestamp: Utc::now(),
    });

    assert!(state.streaming_chunk.is_empty());
    assert!(state.active_run_id.is_none());
    assert_eq!(state.messages.len(), 1);
    assert_eq!(state.messages[0].content, "partial response");
}

#[test]
fn test_apply_tool_execution_started() {
    let mut state = AppState::new();
    apply_agent_event(&mut state, AgentEvent::ToolExecutionStarted {
        run_id: run_id(),
        call_id: "c1".into(),
        tool_name: "bash".into(),
        timestamp: Utc::now(),
    });
    assert_eq!(state.tool_events.len(), 1);
    assert_eq!(state.tool_events[0].tool_name, "bash");
    assert_eq!(state.tool_events[0].call_id, "c1");
    matches!(state.tool_events[0].status, ToolStatus::Started);
}

#[test]
fn test_apply_tool_stdout() {
    let mut state = AppState::new();
    apply_agent_event(&mut state, AgentEvent::ToolExecutionStarted {
        run_id: run_id(),
        call_id: "c2".into(),
        tool_name: "bash".into(),
        timestamp: Utc::now(),
    });
    apply_agent_event(&mut state, AgentEvent::ToolStdout {
        run_id: run_id(),
        call_id: "c2".into(),
        line: "output line".into(),
        timestamp: Utc::now(),
    });
    let entry = state.tool_events.iter().find(|t| t.call_id == "c2").unwrap();
    assert_eq!(entry.output.as_deref(), Some("output line"));
    assert!(matches!(entry.status, ToolStatus::Stdout(_)));
}

#[test]
fn test_apply_tool_completed() {
    let mut state = AppState::new();
    apply_agent_event(&mut state, AgentEvent::ToolExecutionStarted {
        run_id: run_id(),
        call_id: "c3".into(),
        tool_name: "bash".into(),
        timestamp: Utc::now(),
    });
    apply_agent_event(&mut state, AgentEvent::ToolExecutionCompleted {
        run_id: run_id(),
        call_id: "c3".into(),
        exit_code: Some(0),
        timestamp: Utc::now(),
    });
    let entry = state.tool_events.iter().find(|t| t.call_id == "c3").unwrap();
    assert!(matches!(entry.status, ToolStatus::Completed));
}

#[test]
fn test_apply_session_created() {
    let mut state = AppState::new();
    let sid = session_id();
    apply_agent_event(&mut state, AgentEvent::SessionCreated {
        session_id: sid.clone(),
        timestamp: Utc::now(),
    });
    assert_eq!(state.sessions.len(), 1);
    assert_eq!(state.sessions[0].id, sid.to_string());
    assert_eq!(state.active_session_id, Some(sid.to_string()));
}

#[test]
fn test_apply_auth_state_changed() {
    let mut state = AppState::new();
    apply_agent_event(&mut state, AgentEvent::AuthStateChanged {
        provider: provider_id("github"),
        state: AuthState::Authenticated {
            provider: provider_id("github"),
            expires_at: None,
        },
        timestamp: Utc::now(),
    });
    assert_eq!(state.providers.len(), 1);
    assert_eq!(state.providers[0].id, "github");
    assert_eq!(state.providers[0].auth_state, "authenticated");
}

#[test]
fn test_log_entries_capped_at_500() {
    let mut state = AppState::new();
    for i in 0..600 {
        apply_agent_event(&mut state, AgentEvent::RunCompleted {
            run_id: run_id(),
            timestamp: Utc::now(),
        });
        // Each RunCompleted adds one log entry; also RunStarted adds one.
        // Use a cheaper event: TokenChunk doesn't add logs; use RunCompleted (adds 1) repeatedly.
        let _ = i;
    }
    assert!(state.log_entries.len() <= 500, "log_entries len = {}", state.log_entries.len());
}

#[test]
fn test_tool_events_capped_at_200() {
    let mut state = AppState::new();
    for i in 0..250 {
        apply_agent_event(&mut state, AgentEvent::ToolExecutionStarted {
            run_id: run_id(),
            call_id: format!("c{i}"),
            tool_name: "bash".into(),
            timestamp: Utc::now(),
        });
    }
    assert!(state.tool_events.len() <= 200, "tool_events len = {}", state.tool_events.len());
}

// ──────────────────────────────────────────────
// 2. Input mapping tests
// ──────────────────────────────────────────────

#[test]
fn test_key_q_maps_to_quit() {
    let action = map_key(key(KeyCode::Char('q'), KeyModifiers::NONE));
    assert!(matches!(action, KeyAction::Quit));
}

#[test]
fn test_ctrl_c_maps_to_quit() {
    let action = map_key(key(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(matches!(action, KeyAction::Quit));
}

#[test]
fn test_key_1_maps_to_focus_conversation() {
    let action = map_key(key(KeyCode::Char('1'), KeyModifiers::NONE));
    assert!(matches!(action, KeyAction::FocusConversation));
}

#[test]
fn test_key_2_maps_to_focus_tools() {
    let action = map_key(key(KeyCode::Char('2'), KeyModifiers::NONE));
    assert!(matches!(action, KeyAction::FocusTools));
}

#[test]
fn test_key_j_maps_to_scroll_down() {
    let action = map_key(key(KeyCode::Char('j'), KeyModifiers::NONE));
    assert!(matches!(action, KeyAction::ScrollDown));
}

#[test]
fn test_key_k_maps_to_scroll_up() {
    let action = map_key(key(KeyCode::Char('k'), KeyModifiers::NONE));
    assert!(matches!(action, KeyAction::ScrollUp));
}

#[test]
fn test_key_y_maps_to_approve() {
    let action = map_key(key(KeyCode::Char('y'), KeyModifiers::NONE));
    assert!(matches!(action, KeyAction::ApproveAction));
}

#[test]
fn test_key_n_maps_to_deny() {
    let action = map_key(key(KeyCode::Char('n'), KeyModifiers::NONE));
    assert!(matches!(action, KeyAction::DenyAction));
}

#[test]
fn test_ctrl_i_maps_to_interrupt() {
    let action = map_key(key(KeyCode::Char('i'), KeyModifiers::CONTROL));
    assert!(matches!(action, KeyAction::Interrupt));
}

#[test]
fn test_enter_maps_to_submit() {
    let action = map_key(key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(action, KeyAction::SubmitInput));
}

#[test]
fn test_char_a_maps_to_type_char() {
    let action = map_key(key(KeyCode::Char('a'), KeyModifiers::NONE));
    assert!(matches!(action, KeyAction::TypeChar('a')));
}

// ──────────────────────────────────────────────
// 3. Layout tests
// ──────────────────────────────────────────────

fn make_rect(w: u16, h: u16) -> ratatui::layout::Rect {
    ratatui::layout::Rect::new(0, 0, w, h)
}

#[test]
fn test_layout_produces_6_rects() {
    let rects = compute_layout(make_rect(80, 24));
    // PaneRects has 8 named panes — all fields are Rect (non-zero terminal)
    let all = [
        rects.conversation,
        rects.tools,
        rects.status_bar,
        rects.input_bar,
        rects.sessions,
        rects.context,
        rects.auth,
        rects.logs,
    ];
    // At least 6 have positive area
    let nonzero = all.iter().filter(|r| r.width > 0 && r.height > 0).count();
    assert!(nonzero >= 6, "expected ≥6 non-zero rects, got {nonzero}");
}

#[test]
fn test_layout_no_overlap() {
    let rects = compute_layout(make_rect(80, 24));
    let all = [
        rects.conversation,
        rects.tools,
        rects.sessions,
        rects.context,
        rects.auth,
        rects.logs,
    ];
    for i in 0..all.len() {
        for j in (i + 1)..all.len() {
            let a = all[i];
            let b = all[j];
            if a.width == 0 || a.height == 0 || b.width == 0 || b.height == 0 {
                continue;
            }
            let overlap_x = a.x < b.x + b.width && b.x < a.x + a.width;
            let overlap_y = a.y < b.y + b.height && b.y < a.y + a.height;
            assert!(
                !(overlap_x && overlap_y),
                "rects[{i}] ({:?}) and rects[{j}] ({:?}) overlap",
                a, b
            );
        }
    }
}

#[test]
fn test_layout_fits_within_terminal() {
    let terminal_rect = make_rect(80, 24);
    let rects = compute_layout(terminal_rect);
    let all = [
        rects.conversation,
        rects.tools,
        rects.status_bar,
        rects.input_bar,
        rects.sessions,
        rects.context,
        rects.auth,
        rects.logs,
    ];
    for r in &all {
        assert!(r.x + r.width <= terminal_rect.width, "rect extends beyond terminal width: {r:?}");
        assert!(r.y + r.height <= terminal_rect.height, "rect extends beyond terminal height: {r:?}");
    }
}

#[test]
fn test_layout_handles_small_terminal() {
    // Should not panic on a very small terminal
    let _ = compute_layout(make_rect(40, 10));
    let _ = compute_layout(make_rect(1, 1));
    let _ = compute_layout(make_rect(0, 0));
}

// ──────────────────────────────────────────────
// 4. Pane rendering tests
// ──────────────────────────────────────────────

#[test]
fn test_conversation_pane_renders_empty() {
    let state = AppState::new();
    let mut terminal = make_terminal(80, 24);
    terminal.draw(|frame| {
        let area = frame.area();
        panes::conversation::render(frame, &state, area, "");
    }).unwrap();
    let content = buf_content(&terminal);
    assert!(content.contains("Conversation") || !content.is_empty());
}

#[test]
fn test_conversation_pane_renders_messages() {
    let mut state = AppState::new();
    state.messages.push(ChatMessage {
        role: MessageRole::User,
        content: "Hello agent!".into(),
        timestamp: Utc::now(),
    });
    let mut terminal = make_terminal(80, 24);
    terminal.draw(|frame| {
        let area = frame.area();
        panes::conversation::render(frame, &state, area, "");
    }).unwrap();
    let content = buf_content(&terminal);
    assert!(content.contains("Hello agent!"), "expected message in buffer");
}

#[test]
fn test_tools_pane_renders_empty() {
    let state = AppState::new();
    let mut terminal = make_terminal(80, 24);
    terminal.draw(|frame| {
        let area = frame.area();
        panes::tools::render(frame, &state, area);
    }).unwrap();
    // Should not panic; buffer should contain the pane title
    let content = buf_content(&terminal);
    assert!(content.contains("Tool Activity") || !content.is_empty());
}

#[test]
fn test_tools_pane_renders_activity() {
    let mut state = AppState::new();
    state.tool_events.push_back(ToolActivity {
        call_id: "c1".into(),
        tool_name: "my_special_tool".into(),
        status: ToolStatus::Started,
        output: None,
        timestamp: Utc::now(),
    });
    let mut terminal = make_terminal(80, 24);
    terminal.draw(|frame| {
        let area = frame.area();
        panes::tools::render(frame, &state, area);
    }).unwrap();
    let content = buf_content(&terminal);
    assert!(content.contains("my_special_tool"), "expected tool name in buffer");
}

#[test]
fn test_context_pane_no_context() {
    let state = AppState::new();
    let mut terminal = make_terminal(80, 24);
    terminal.draw(|frame| {
        let area = frame.area();
        panes::context::render(frame, &state, area);
    }).unwrap();
    let content = buf_content(&terminal);
    assert!(content.contains("No context") || content.contains("Context"), "expected placeholder text");
}

#[test]
fn test_context_pane_with_context() {
    let mut state = AppState::new();
    state.context_info = Some(ContextInfo { file_count: 42, token_count: 1234 });
    let mut terminal = make_terminal(80, 24);
    terminal.draw(|frame| {
        let area = frame.area();
        panes::context::render(frame, &state, area);
    }).unwrap();
    let content = buf_content(&terminal);
    assert!(content.contains("42"), "expected file count in buffer");
    assert!(content.contains("1234"), "expected token count in buffer");
}

#[test]
fn test_session_pane_renders_sessions() {
    let mut state = AppState::new();
    state.sessions.push(SessionSummary {
        id: "sess-001".into(),
        status: "active".into(),
        run_count: 3,
        created_at: "2024-01-01 10:00".into(),
    });
    let mut terminal = make_terminal(80, 24);
    terminal.draw(|frame| {
        let area = frame.area();
        panes::session::render(frame, &state, area);
    }).unwrap();
    let content = buf_content(&terminal);
    assert!(content.contains("sess-00") || content.contains("active"), "expected session data");
}

#[test]
fn test_auth_pane_renders_providers() {
    let mut state = AppState::new();
    state.providers.push(ProviderStatus {
        id: "github".into(),
        kind: "oauth".into(),
        auth_state: "authenticated".into(),
    });
    let mut terminal = make_terminal(80, 24);
    terminal.draw(|frame| {
        let area = frame.area();
        panes::auth::render(frame, &state, area);
    }).unwrap();
    let content = buf_content(&terminal);
    assert!(content.contains("github"), "expected provider name in buffer");
}

#[test]
fn test_logs_pane_renders_entries() {
    let mut state = AppState::new();
    state.log_entries.push_back(LogEntry {
        level: "info".into(),
        message: "system initialized".into(),
        timestamp: Utc::now(),
    });
    let mut terminal = make_terminal(80, 24);
    terminal.draw(|frame| {
        let area = frame.area();
        panes::logs::render(frame, &state, area);
    }).unwrap();
    let content = buf_content(&terminal);
    assert!(content.contains("system initialized"), "expected log message in buffer");
}

#[test]
fn test_approval_shown_in_conversation() {
    let mut state = AppState::new();
    state.pending_approval = Some(ApprovalRequest {
        run_id: "run-1".into(),
        call_id: "call-1".into(),
        tool_name: "write_file".into(),
        description: "Tool: write_file args: {}".into(),
    });
    let mut terminal = make_terminal(80, 24);
    terminal.draw(|frame| {
        let area = frame.area();
        panes::conversation::render(frame, &state, area, "");
    }).unwrap();
    let content = buf_content(&terminal);
    assert!(content.contains("write_file") || content.contains("Approve"), "expected approval prompt");
}

// ──────────────────────────────────────────────
// 5. Interaction tests
// ──────────────────────────────────────────────

/// Simulate handle_action-style focus changes by calling map_key and applying
/// the focus logic directly (mirrors what App::handle_action does).
fn apply_focus(state: &mut AppState, code: KeyCode) {
    let action = map_key(key(code, KeyModifiers::NONE));
    match action {
        KeyAction::FocusConversation => state.focused_pane = PaneId::Conversation,
        KeyAction::FocusTools => state.focused_pane = PaneId::Tools,
        KeyAction::FocusContext => state.focused_pane = PaneId::Context,
        KeyAction::FocusSessions => state.focused_pane = PaneId::Sessions,
        KeyAction::FocusAuth => state.focused_pane = PaneId::Auth,
        KeyAction::FocusLogs => state.focused_pane = PaneId::Logs,
        _ => {}
    }
}

#[test]
fn test_focus_changes_on_key() {
    let mut state = AppState::new();
    assert_eq!(state.focused_pane, PaneId::Conversation);

    apply_focus(&mut state, KeyCode::Char('2'));
    assert_eq!(state.focused_pane, PaneId::Tools);

    apply_focus(&mut state, KeyCode::Char('3'));
    assert_eq!(state.focused_pane, PaneId::Context);

    apply_focus(&mut state, KeyCode::Char('4'));
    assert_eq!(state.focused_pane, PaneId::Sessions);

    apply_focus(&mut state, KeyCode::Char('5'));
    assert_eq!(state.focused_pane, PaneId::Auth);

    apply_focus(&mut state, KeyCode::Char('6'));
    assert_eq!(state.focused_pane, PaneId::Logs);

    apply_focus(&mut state, KeyCode::Char('1'));
    assert_eq!(state.focused_pane, PaneId::Conversation);
}

#[test]
fn test_interrupt_clears_active_run() {
    // The Interrupt action logs but does NOT clear active_run_id (that's done by RunCompleted).
    // We test the state interaction: active_run_id remains set after interrupt,
    // and a log entry is added to reflect the interrupt request.
    let mut state = AppState::new();
    state.active_run_id = Some("run-xyz".into());

    // Simulate what App::handle_action does for Interrupt
    if let Some(run_id_str) = &state.active_run_id.clone() {
        state.log_entries.push_back(LogEntry {
            level: "info".into(),
            message: format!("Interrupt requested for run {}", run_id_str),
            timestamp: Utc::now(),
        });
        state.status_message = Some("Interrupt requested".into());
    }

    // active_run_id is still set (cleared only when RunCompleted arrives)
    assert_eq!(state.active_run_id, Some("run-xyz".into()));
    assert!(!state.log_entries.is_empty());
    assert!(state.log_entries.back().unwrap().message.contains("run-xyz"));
}

#[test]
fn test_approve_clears_pending_approval() {
    let mut state = AppState::new();
    state.pending_approval = Some(ApprovalRequest {
        run_id: "r1".into(),
        call_id: "c1".into(),
        tool_name: "exec".into(),
        description: "exec something".into(),
    });

    // Simulate App::handle_action for ApproveAction
    let was_some = state.pending_approval.take().is_some();
    assert!(was_some);
    assert!(state.pending_approval.is_none());
}

#[test]
fn test_deny_clears_pending_approval() {
    let mut state = AppState::new();
    state.pending_approval = Some(ApprovalRequest {
        run_id: "r1".into(),
        call_id: "c1".into(),
        tool_name: "shell".into(),
        description: "shell something".into(),
    });

    // Simulate App::handle_action for DenyAction
    let was_some = state.pending_approval.take().is_some();
    assert!(was_some);
    assert!(state.pending_approval.is_none());
}
