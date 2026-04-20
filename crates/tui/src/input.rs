use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone)]
pub enum KeyAction {
    Quit,
    FocusConversation,
    FocusTools,
    FocusContext,
    FocusSessions,
    FocusAuth,
    FocusLogs,
    ScrollUp,
    ScrollDown,
    PageUp,
    PageDown,
    Interrupt,
    ApproveAction,
    DenyAction,
    SubmitInput,
    TypeChar(char),
    Backspace,
    Help,
    None,
}

pub fn map_key(key: KeyEvent) -> KeyAction {
    match (key.code, key.modifiers) {
        (KeyCode::Char('q'), KeyModifiers::NONE) => KeyAction::Quit,
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => KeyAction::Quit,
        (KeyCode::Char('1'), _) => KeyAction::FocusConversation,
        (KeyCode::Char('2'), _) => KeyAction::FocusTools,
        (KeyCode::Char('3'), _) => KeyAction::FocusContext,
        (KeyCode::Char('4'), _) => KeyAction::FocusSessions,
        (KeyCode::Char('5'), _) => KeyAction::FocusAuth,
        (KeyCode::Char('6'), _) => KeyAction::FocusLogs,
        (KeyCode::Up, _) | (KeyCode::Char('k'), KeyModifiers::NONE) => KeyAction::ScrollUp,
        (KeyCode::Down, _) | (KeyCode::Char('j'), KeyModifiers::NONE) => KeyAction::ScrollDown,
        (KeyCode::PageUp, _) => KeyAction::PageUp,
        (KeyCode::PageDown, _) => KeyAction::PageDown,
        (KeyCode::Char('i'), KeyModifiers::CONTROL) => KeyAction::Interrupt,
        (KeyCode::Enter, _) => KeyAction::SubmitInput,
        (KeyCode::Backspace, _) => KeyAction::Backspace,
        (KeyCode::Char('?'), _) => KeyAction::Help,
        (KeyCode::Char(c), KeyModifiers::NONE) => KeyAction::TypeChar(c),
        (KeyCode::Char(c), KeyModifiers::SHIFT) => KeyAction::TypeChar(c),
        _ => KeyAction::None,
    }
}
