use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

use crate::layout::border_style;
use crate::state::{AppState, PaneId};

pub fn render(frame: &mut Frame, state: &AppState, area: Rect) {
    let bstyle = border_style(&PaneId::Sessions, &state.focused_pane);

    let items: Vec<ListItem> = state.sessions.iter().enumerate().map(|(i, s)| {
        let is_active = state.active_session_id.as_deref() == Some(&s.id);
        let is_cursor = state.session_list_cursor == i;
        let marker = if is_active { "●" } else { " " };
        let id_short = &s.id[..s.id.len().min(8)];
        let line = Line::from(vec![
            Span::styled(
                format!("{} {}  ", marker, id_short),
                if is_active {
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                },
            ),
            Span::raw(format!("{} runs:{}", s.status, s.run_count)),
        ]);
        if is_cursor {
            ListItem::new(line).style(Style::default().bg(Color::DarkGray))
        } else {
            ListItem::new(line)
        }
    }).collect();

    let block = Block::default()
        .title(" Sessions [4] ")
        .borders(Borders::ALL)
        .border_style(bstyle);

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}
