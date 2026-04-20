use ratatui::layout::{Constraint, Direction, Layout, Rect};
use crate::state::PaneId;
use crate::theme::Theme;

pub struct PaneRects {
    pub conversation: Rect,
    pub tools: Rect,
    pub input_bar: Rect,
    pub status_bar: Rect,
    pub sessions: Rect,
    pub context: Rect,
    pub data_sources: Rect,
    pub auth: Rect,
    pub logs: Rect,
}

pub fn compute_layout(area: Rect) -> PaneRects {
    let top_pct = 65u16;
    let remaining = area.height.saturating_sub(3);
    let top_h = (area.height as u32 * top_pct as u32 / 100).min(remaining as u32) as u16;
    let bottom_h = remaining.saturating_sub(top_h);

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(top_h),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(bottom_h),
        ])
        .split(area);

    let top_area = vertical[0];
    let status_bar = vertical[1];
    let input_bar = vertical[2];
    let bottom_area = vertical[3];

    // Top: conversation (left 60%) | right column (40%)
    let top_split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(top_area);

    // Right column: Tools (top 50%) | Logs (bottom 50%)
    let right_split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(top_split[1]);

    // Bottom strip: Sessions | Context | DataSources | Auth
    let bottom_split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(35),
            Constraint::Percentage(15),
        ])
        .split(bottom_area);

    PaneRects {
        conversation: top_split[0],
        tools: right_split[0],
        logs: right_split[1],
        status_bar,
        input_bar,
        sessions: bottom_split[0],
        context: bottom_split[1],
        data_sources: bottom_split[2],
        auth: bottom_split[3],
    }
}

pub fn border_style(pane: &PaneId, focused: &PaneId, theme: &Theme) -> ratatui::style::Style {
    use ratatui::style::Style;
    if pane == focused {
        Style::default().fg(theme.border_focused)
    } else {
        Style::default().fg(theme.border_unfocused)
    }
}
