use ratatui::style::Color;

/// All color slots used throughout the TUI.
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: &'static str,

    // Chrome
    pub border_focused: Color,
    pub border_unfocused: Color,
    pub status_bar_bg: Color,
    pub status_bar_fg: Color,

    // General text
    pub text_primary: Color,
    pub text_dim: Color,
    pub text_timestamp: Color,

    // Conversation roles
    pub role_user: Color,
    pub role_assistant: Color,
    pub role_system: Color,
    pub role_tool: Color,
    pub streaming_cursor: Color,
    pub thinking: Color,

    // Approval popup
    pub approval_bg: Color,
    pub approval_fg: Color,

    // Tool activity
    pub tool_started: Color,
    pub tool_done: Color,
    pub tool_failed: Color,
    pub tool_stderr: Color,
    pub tool_cancelled: Color,

    // Auth
    pub auth_ok: Color,
    pub auth_pending: Color,
    pub auth_failed: Color,

    // Log levels
    pub log_error: Color,
    pub log_warn: Color,
    pub log_info: Color,
    pub log_debug: Color,

    // Data source badges
    pub ds_obsidian: Color,
    pub ds_qdrant: Color,
    pub ds_postgres: Color,
    pub ds_other: Color,

    // Context pane labels
    pub label: Color,

    // Session list selection highlight
    pub selected_bg: Color,
    pub selected_fg: Color,
}

// ─── Theme definitions ────────────────────────────────────────────────────────

/// Matrix green — the primary theme.
/// Inspired by: https://pixflow.net/blog/the-green-color-scheme-of-the-matrix/
pub fn matrix() -> Theme {
    const BRIGHT: Color = Color::Rgb(0, 255, 65);   // #00FF41 — hot green
    const MID:    Color = Color::Rgb(0, 175, 30);    // #00AF1E — medium green
    const DIM:    Color = Color::Rgb(0, 95, 15);     // #005F0F — dim green
    const DARK:   Color = Color::Rgb(0, 40, 5);      // #002805 — near black green
    const BLACK:  Color = Color::Black;

    Theme {
        name: "Matrix",

        border_focused:   BRIGHT,
        border_unfocused: DIM,
        status_bar_bg:    DARK,
        status_bar_fg:    BRIGHT,

        text_primary:    BRIGHT,
        text_dim:        DIM,
        text_timestamp:  DIM,

        role_user:      BRIGHT,
        role_assistant: MID,
        role_system:    Color::Rgb(200, 255, 200),
        role_tool:      Color::Rgb(0, 200, 50),
        streaming_cursor: BRIGHT,
        thinking:       DIM,

        approval_bg: DARK,
        approval_fg: BRIGHT,

        tool_started:   Color::Rgb(0, 200, 50),
        tool_done:      BRIGHT,
        tool_failed:    Color::Rgb(255, 80, 80),
        tool_stderr:    Color::Rgb(200, 100, 0),
        tool_cancelled: DIM,

        auth_ok:      BRIGHT,
        auth_pending: Color::Rgb(200, 200, 0),
        auth_failed:  Color::Rgb(255, 80, 80),

        log_error: Color::Rgb(255, 80, 80),
        log_warn:  Color::Rgb(200, 200, 0),
        log_info:  MID,
        log_debug: DIM,

        ds_obsidian: Color::Rgb(180, 255, 100),
        ds_qdrant:   BRIGHT,
        ds_postgres: MID,
        ds_other:    Color::Rgb(0, 200, 100),

        label:       MID,
        selected_bg: DARK,
        selected_fg: BRIGHT,
    }
}

/// Classic — the original cyan-on-black look.
pub fn classic() -> Theme {
    Theme {
        name: "Classic",

        border_focused:   Color::Cyan,
        border_unfocused: Color::DarkGray,
        status_bar_bg:    Color::DarkGray,
        status_bar_fg:    Color::White,

        text_primary:    Color::White,
        text_dim:        Color::DarkGray,
        text_timestamp:  Color::DarkGray,

        role_user:      Color::Green,
        role_assistant: Color::Cyan,
        role_system:    Color::Yellow,
        role_tool:      Color::Magenta,
        streaming_cursor: Color::Cyan,
        thinking:       Color::DarkGray,

        approval_bg: Color::DarkGray,
        approval_fg: Color::White,

        tool_started:   Color::Yellow,
        tool_done:      Color::Green,
        tool_failed:    Color::Red,
        tool_stderr:    Color::Red,
        tool_cancelled: Color::DarkGray,

        auth_ok:      Color::Green,
        auth_pending: Color::Yellow,
        auth_failed:  Color::Red,

        log_error: Color::Red,
        log_warn:  Color::Yellow,
        log_info:  Color::White,
        log_debug: Color::DarkGray,

        ds_obsidian: Color::Magenta,
        ds_qdrant:   Color::Yellow,
        ds_postgres: Color::Cyan,
        ds_other:    Color::Green,

        label:       Color::Yellow,
        selected_bg: Color::DarkGray,
        selected_fg: Color::White,
    }
}

/// Dracula — purple/pink dark theme.
pub fn dracula() -> Theme {
    Theme {
        name: "Dracula",

        border_focused:   Color::Rgb(189, 147, 249),  // purple
        border_unfocused: Color::Rgb(68, 71, 90),     // comment
        status_bar_bg:    Color::Rgb(40, 42, 54),     // background
        status_bar_fg:    Color::Rgb(248, 248, 242),  // foreground

        text_primary:    Color::Rgb(248, 248, 242),
        text_dim:        Color::Rgb(98, 114, 164),
        text_timestamp:  Color::Rgb(98, 114, 164),

        role_user:      Color::Rgb(80, 250, 123),     // green
        role_assistant: Color::Rgb(139, 233, 253),    // cyan
        role_system:    Color::Rgb(255, 184, 108),    // orange
        role_tool:      Color::Rgb(255, 121, 198),    // pink
        streaming_cursor: Color::Rgb(189, 147, 249),
        thinking:       Color::Rgb(98, 114, 164),

        approval_bg: Color::Rgb(40, 42, 54),
        approval_fg: Color::Rgb(255, 184, 108),

        tool_started:   Color::Rgb(255, 184, 108),
        tool_done:      Color::Rgb(80, 250, 123),
        tool_failed:    Color::Rgb(255, 85, 85),
        tool_stderr:    Color::Rgb(255, 85, 85),
        tool_cancelled: Color::Rgb(98, 114, 164),

        auth_ok:      Color::Rgb(80, 250, 123),
        auth_pending: Color::Rgb(255, 184, 108),
        auth_failed:  Color::Rgb(255, 85, 85),

        log_error: Color::Rgb(255, 85, 85),
        log_warn:  Color::Rgb(255, 184, 108),
        log_info:  Color::Rgb(248, 248, 242),
        log_debug: Color::Rgb(98, 114, 164),

        ds_obsidian: Color::Rgb(255, 121, 198),
        ds_qdrant:   Color::Rgb(255, 184, 108),
        ds_postgres: Color::Rgb(139, 233, 253),
        ds_other:    Color::Rgb(80, 250, 123),

        label:       Color::Rgb(255, 184, 108),
        selected_bg: Color::Rgb(68, 71, 90),
        selected_fg: Color::Rgb(248, 248, 242),
    }
}

/// Nord — cool arctic blue theme.
pub fn nord() -> Theme {
    Theme {
        name: "Nord",

        border_focused:   Color::Rgb(136, 192, 208),  // nord8 frost
        border_unfocused: Color::Rgb(67, 76, 94),     // nord2 polar night
        status_bar_bg:    Color::Rgb(46, 52, 64),     // nord0
        status_bar_fg:    Color::Rgb(216, 222, 233),  // nord4

        text_primary:    Color::Rgb(229, 233, 240),   // nord5
        text_dim:        Color::Rgb(76, 86, 106),     // nord3
        text_timestamp:  Color::Rgb(76, 86, 106),

        role_user:      Color::Rgb(163, 190, 140),    // nord14 green
        role_assistant: Color::Rgb(136, 192, 208),    // nord8 frost
        role_system:    Color::Rgb(235, 203, 139),    // nord13 yellow
        role_tool:      Color::Rgb(180, 142, 173),    // nord15 purple
        streaming_cursor: Color::Rgb(136, 192, 208),
        thinking:       Color::Rgb(76, 86, 106),

        approval_bg: Color::Rgb(46, 52, 64),
        approval_fg: Color::Rgb(235, 203, 139),

        tool_started:   Color::Rgb(235, 203, 139),
        tool_done:      Color::Rgb(163, 190, 140),
        tool_failed:    Color::Rgb(191, 97, 106),     // nord11 red
        tool_stderr:    Color::Rgb(208, 135, 112),    // nord12 orange
        tool_cancelled: Color::Rgb(76, 86, 106),

        auth_ok:      Color::Rgb(163, 190, 140),
        auth_pending: Color::Rgb(235, 203, 139),
        auth_failed:  Color::Rgb(191, 97, 106),

        log_error: Color::Rgb(191, 97, 106),
        log_warn:  Color::Rgb(235, 203, 139),
        log_info:  Color::Rgb(229, 233, 240),
        log_debug: Color::Rgb(76, 86, 106),

        ds_obsidian: Color::Rgb(180, 142, 173),
        ds_qdrant:   Color::Rgb(235, 203, 139),
        ds_postgres: Color::Rgb(136, 192, 208),
        ds_other:    Color::Rgb(163, 190, 140),

        label:       Color::Rgb(235, 203, 139),
        selected_bg: Color::Rgb(67, 76, 94),
        selected_fg: Color::Rgb(229, 233, 240),
    }
}

/// All available themes in display order.
pub fn all_themes() -> Vec<Theme> {
    vec![matrix(), classic(), dracula(), nord()]
}

/// Resolve a theme by name (case-insensitive). Falls back to Matrix.
pub fn by_name(name: &str) -> Theme {
    match name.to_lowercase().as_str() {
        "classic"           => classic(),
        "dracula"           => dracula(),
        "nord"              => nord(),
        _                   => matrix(), // "matrix", "default", or unknown → Matrix
    }
}
