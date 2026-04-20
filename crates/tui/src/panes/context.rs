use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::layout::border_style;
use crate::state::{AppState, PaneId};

pub fn render(frame: &mut Frame, state: &AppState, area: Rect) {
    let theme = &state.theme;
    let bstyle = border_style(&PaneId::Context, &state.focused_pane, theme);
    let block = Block::default()
        .title(" Context [3] ")
        .borders(Borders::ALL)
        .border_style(bstyle);

    let text = match &state.context_info {
        None => vec![Line::from(Span::styled("No context loaded", Style::default().fg(theme.text_dim)))],
        Some(info) => vec![
            Line::from(vec![
                Span::styled("Files:  ", Style::default().fg(theme.label)),
                Span::styled(info.file_count.to_string(), Style::default().fg(theme.text_primary)),
            ]),
            Line::from(vec![
                Span::styled("Tokens: ", Style::default().fg(theme.label)),
                Span::styled(info.token_count.to_string(), Style::default().fg(theme.text_primary)),
            ]),
        ],
    };

    let para = Paragraph::new(text).block(block);
    frame.render_widget(para, area);
}
