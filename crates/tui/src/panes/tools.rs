use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

use crate::layout::border_style;
use crate::state::{AppState, PaneId, ToolStatus};

pub fn render(frame: &mut Frame, state: &AppState, area: Rect) {
    let bstyle = border_style(&PaneId::Tools, &state.focused_pane);

    let visible: Vec<_> = state.tool_events.iter().rev().take(20).collect();

    let items: Vec<ListItem> = visible.iter().rev().map(|t| {
        let (status_str, color) = match &t.status {
            ToolStatus::Started => ("[started]  ", Color::Yellow),
            ToolStatus::Stdout(_) => ("[stdout]   ", Color::White),
            ToolStatus::Stderr(_) => ("[stderr]   ", Color::Red),
            ToolStatus::Completed => ("[done]     ", Color::Green),
            ToolStatus::Failed(_) => ("[failed]   ", Color::Red),
            ToolStatus::Cancelled => ("[cancelled]", Color::DarkGray),
        };
        let extra = match &t.status {
            ToolStatus::Stdout(s) | ToolStatus::Stderr(s) => format!(": {}", s),
            ToolStatus::Failed(r) => format!(": {}", r),
            _ => String::new(),
        };
        let ts = t.timestamp.format("%H:%M:%S").to_string();
        let line = Line::from(vec![
            Span::styled(format!("{} ", ts), Style::default().fg(Color::DarkGray)),
            Span::styled(status_str, Style::default().fg(color).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {}", t.tool_name), Style::default().fg(Color::White)),
            Span::styled(extra, Style::default().fg(Color::Gray)),
        ]);
        ListItem::new(line)
    }).collect();

    let block = Block::default()
        .title(" Tool Activity [2] ")
        .borders(Borders::ALL)
        .border_style(bstyle);

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}
