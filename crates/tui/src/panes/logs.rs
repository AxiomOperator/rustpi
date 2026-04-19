use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

use crate::layout::border_style;
use crate::state::{AppState, PaneId};

pub fn render(frame: &mut Frame, state: &AppState, area: Rect) {
    let bstyle = border_style(&PaneId::Logs, &state.focused_pane);

    let visible_height = area.height.saturating_sub(2) as usize;
    let entries: Vec<_> = state.log_entries.iter().rev().take(visible_height).collect();

    let items: Vec<ListItem> = entries.iter().rev().map(|e| {
        let color = match e.level.as_str() {
            "error" => Color::Red,
            "warn" => Color::Yellow,
            "info" => Color::White,
            _ => Color::DarkGray,
        };
        let ts = e.timestamp.format("%H:%M:%S").to_string();
        let line = Line::from(vec![
            Span::styled(format!("{} ", ts), Style::default().fg(Color::DarkGray)),
            Span::styled(format!("[{}] ", e.level.to_uppercase()), Style::default().fg(color)),
            Span::styled(e.message.clone(), Style::default().fg(color)),
        ]);
        ListItem::new(line)
    }).collect();

    let block = Block::default()
        .title(" Logs [6] ")
        .borders(Borders::ALL)
        .border_style(bstyle);

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}
