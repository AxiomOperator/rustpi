use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::layout::border_style;
use crate::state::{AppState, PaneId};

pub fn render(frame: &mut Frame, state: &AppState, area: Rect) {
    let theme = &state.theme;
    let bstyle = border_style(&PaneId::Auth, &state.focused_pane, theme);
    let block = Block::default()
        .title(" Auth [5] ")
        .borders(Borders::ALL)
        .border_style(bstyle);

    if state.providers.is_empty() {
        let para = Paragraph::new("No providers configured")
            .style(Style::default().fg(theme.text_dim))
            .block(block);
        frame.render_widget(para, area);
        return;
    }

    let items: Vec<ListItem> = state.providers.iter().map(|p| {
        let color = match p.auth_state.as_str() {
            "authenticated"          => theme.auth_ok,
            "pending"                => theme.auth_pending,
            "expired" | "failed"     => theme.auth_failed,
            _                        => theme.text_dim,
        };
        let dot = match p.auth_state.as_str() {
            "authenticated" => "●",
            "pending"       => "◐",
            "expired" | "failed" => "✗",
            _               => "○",
        };
        let line = Line::from(vec![
            Span::styled(format!("{} ", dot), Style::default().fg(color).add_modifier(Modifier::BOLD)),
            Span::styled(p.id.clone(), Style::default().fg(theme.text_primary)),
            Span::styled(format!(" [{}]", p.auth_state), Style::default().fg(color)),
        ]);
        ListItem::new(line)
    }).collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}
