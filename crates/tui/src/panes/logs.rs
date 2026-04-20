use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

use crate::layout::border_style;
use crate::state::{AppState, PaneId};

pub fn render(frame: &mut Frame, state: &AppState, area: Rect) {
    let theme = &state.theme;
    let bstyle = border_style(&PaneId::Logs, &state.focused_pane, theme);

    let inner_width = area.width.saturating_sub(2) as usize;
    let visible_height = area.height.saturating_sub(2) as usize;

    // Build wrapped lines, newest last; take only what fits.
    let mut all_lines: Vec<Line> = Vec::new();
    for e in state.log_entries.iter() {
        let color = match e.level.as_str() {
            "error" => theme.log_error,
            "warn"  => theme.log_warn,
            "info"  => theme.log_info,
            _       => theme.log_debug,
        };
        let ts = e.timestamp.format("%H:%M:%S").to_string();
        let prefix = format!("{} [{}] ", ts, e.level.to_uppercase());
        let prefix_len = prefix.len();
        let msg = &e.message;

        // First line gets timestamp + level prefix
        all_lines.push(Line::from(vec![
            Span::styled(ts + " ", Style::default().fg(theme.text_timestamp)),
            Span::styled(
                format!("[{}] ", e.level.to_uppercase()),
                Style::default().fg(color),
            ),
            Span::styled(
                msg.chars().take(inner_width.saturating_sub(prefix_len)).collect::<String>(),
                Style::default().fg(color),
            ),
        ]));

        // Continuation lines for long messages
        if inner_width > prefix_len && msg.len() > inner_width.saturating_sub(prefix_len) {
            let rest = &msg[inner_width.saturating_sub(prefix_len).min(msg.len())..];
            let wrap_width = inner_width.saturating_sub(2); // indent
            let mut pos = 0;
            while pos < rest.len() && all_lines.len() < visible_height * 4 {
                let end = (pos + wrap_width).min(rest.len());
                all_lines.push(Line::from(vec![
                    Span::styled(
                        format!("  {}", &rest[pos..end]),
                        Style::default().fg(color),
                    ),
                ]));
                pos = end;
            }
        }
    }

    // Show the last `visible_height` lines (newest entries at bottom)
    let skip = all_lines.len().saturating_sub(visible_height);
    let items: Vec<ListItem> = all_lines
        .into_iter()
        .skip(skip)
        .map(ListItem::new)
        .collect();

    let block = Block::default()
        .title(" Logs [6] ")
        .borders(Borders::ALL)
        .border_style(bstyle);

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}
