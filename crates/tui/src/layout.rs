use ratatui::layout::{Constraint, Direction, Layout, Rect};
use crate::state::PaneId;

pub struct PaneRects {
    pub conversation: Rect,
    pub tools: Rect,
    pub input_bar: Rect,
    pub status_bar: Rect,
    pub sessions: Rect,
    pub context: Rect,
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

    let top_split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(top_area);

    let bottom_split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(bottom_area);

    PaneRects {
        conversation: top_split[0],
        tools: top_split[1],
        status_bar,
        input_bar,
        sessions: bottom_split[0],
        context: bottom_split[1],
        auth: bottom_split[2],
        logs: bottom_split[3],
    }
}

pub fn border_style(pane: &PaneId, focused: &PaneId) -> ratatui::style::Style {
    use ratatui::style::{Color, Style};
    if pane == focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}
