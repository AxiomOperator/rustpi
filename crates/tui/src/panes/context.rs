use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::layout::border_style;
use crate::state::{AppState, PaneId};

pub fn render(frame: &mut Frame, state: &AppState, area: Rect) {
    let bstyle = border_style(&PaneId::Context, &state.focused_pane);
    let block = Block::default()
        .title(" Context [3] ")
        .borders(Borders::ALL)
        .border_style(bstyle);

    let text = match &state.context_info {
        None => vec![Line::from(Span::styled("No context loaded", Style::default().fg(Color::DarkGray)))],
        Some(info) => vec![
            Line::from(vec![
                Span::styled("Files:  ", Style::default().fg(Color::Yellow)),
                Span::raw(info.file_count.to_string()),
            ]),
            Line::from(vec![
                Span::styled("Tokens: ", Style::default().fg(Color::Yellow)),
                Span::raw(info.token_count.to_string()),
            ]),
        ],
    };

    let para = Paragraph::new(text).block(block);
    frame.render_widget(para, area);
}
