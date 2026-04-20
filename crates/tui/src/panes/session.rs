use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

use crate::layout::border_style;
use crate::state::{AppState, PaneId};

pub fn render(frame: &mut Frame, state: &AppState, area: Rect) {
    let theme = &state.theme;
    let bstyle = border_style(&PaneId::Sessions, &state.focused_pane, theme);

    let items: Vec<ListItem> = state.sessions.iter().enumerate().map(|(i, s)| {
        let is_active = state.active_session_id.as_deref() == Some(&s.id);
        let is_cursor = state.session_list_cursor == i;
        let marker = if is_active { "●" } else { " " };
        let id_short = &s.id[..s.id.len().min(8)];
        let line = Line::from(vec![
            Span::styled(
                format!("{} {}  ", marker, id_short),
                if is_active {
                    Style::default().fg(theme.auth_ok).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.text_dim)
                },
            ),
            Span::styled(
                format!("{} runs:{}", s.status, s.run_count),
                Style::default().fg(theme.text_primary),
            ),
        ]);
        if is_cursor {
            ListItem::new(line).style(Style::default().bg(theme.selected_bg).fg(theme.selected_fg))
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
