use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::layout::border_style;
use crate::state::{AppState, MessageRole, PaneId};

/// Word-wrap `content` to `max_width` characters, returning one string per line.
fn wrap_text(content: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![content.to_string()];
    }
    let mut lines = Vec::new();
    for raw_line in content.split('\n') {
        if raw_line.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in raw_line.split_whitespace() {
            if current.is_empty() {
                if word.len() > max_width {
                    let mut pos = 0;
                    while pos < word.len() {
                        let end = (pos + max_width).min(word.len());
                        lines.push(word[pos..end].to_string());
                        pos = end;
                    }
                } else {
                    current.push_str(word);
                }
            } else if current.len() + 1 + word.len() <= max_width {
                current.push(' ');
                current.push_str(word);
            } else {
                lines.push(current.clone());
                current.clear();
                if word.len() > max_width {
                    let mut pos = 0;
                    while pos < word.len() {
                        let end = (pos + max_width).min(word.len());
                        lines.push(word[pos..end].to_string());
                        pos = end;
                    }
                } else {
                    current.push_str(word);
                }
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

pub fn render(frame: &mut Frame, state: &AppState, area: Rect, input_buffer: &str) {
    let theme = &state.theme;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(area);

    let msg_area = chunks[0];
    let input_area = chunks[1];

    let bstyle = border_style(&PaneId::Conversation, &state.focused_pane, theme);
    let mut items: Vec<ListItem> = state.messages.iter().map(|m| {
        let (prefix, color) = match m.role {
            MessageRole::User      => ("[You]    ", theme.role_user),
            MessageRole::Assistant => ("[Agent]  ", theme.role_assistant),
            MessageRole::System    => ("[System] ", theme.role_system),
            MessageRole::Tool      => ("[Tool]   ", theme.role_tool),
        };
        let ts = m.timestamp.format("%H:%M").to_string();
        let header_len = 6 + prefix.len();
        let content_width = (msg_area.width as usize).saturating_sub(2 + header_len);
        let wrapped = wrap_text(&m.content, content_width);
        let indent = " ".repeat(header_len);
        let ts_style = Style::default().fg(theme.text_timestamp);
        let prefix_style = Style::default().fg(color).add_modifier(Modifier::BOLD);
        let mut lines: Vec<Line> = Vec::with_capacity(wrapped.len());
        for (i, part) in wrapped.into_iter().enumerate() {
            if i == 0 {
                lines.push(Line::from(vec![
                    Span::styled(format!("{} ", ts), ts_style),
                    Span::styled(prefix, prefix_style),
                    Span::raw(part),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::raw(indent.clone()),
                    Span::raw(part),
                ]));
            }
        }
        ListItem::new(Text::from(lines))
    }).collect();

    if state.is_thinking && state.streaming_chunk.is_empty() {
        const FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let frame_idx = (chrono::Utc::now().timestamp_millis() / 250) as usize % FRAMES.len();
        let spinner = FRAMES[frame_idx];
        let ts = chrono::Utc::now().format("%H:%M").to_string();
        let line = Line::from(vec![
            Span::styled(format!("{} ", ts), Style::default().fg(theme.text_timestamp)),
            Span::styled("[Agent]  ", Style::default().fg(theme.role_assistant).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{} thinking…", spinner), Style::default().fg(theme.thinking).add_modifier(Modifier::ITALIC)),
        ]);
        items.push(ListItem::new(line));
    }

    if !state.streaming_chunk.is_empty() {
        let ts = chrono::Utc::now().format("%H:%M").to_string();
        let header_len = 6 + "[Agent]  ".len();
        let content_width = (msg_area.width as usize).saturating_sub(2 + header_len);
        let wrapped = wrap_text(&state.streaming_chunk, content_width);
        let indent = " ".repeat(header_len);
        let ts_style = Style::default().fg(theme.text_timestamp);
        let prefix_style = Style::default().fg(theme.role_assistant).add_modifier(Modifier::BOLD);
        let cursor_style = Style::default().fg(theme.streaming_cursor).add_modifier(Modifier::SLOW_BLINK);
        let mut lines: Vec<Line> = Vec::new();
        let last = wrapped.len().saturating_sub(1);
        for (i, part) in wrapped.into_iter().enumerate() {
            if i == 0 && i == last {
                lines.push(Line::from(vec![
                    Span::styled(format!("{} ", ts), ts_style),
                    Span::styled("[Agent]  ", prefix_style),
                    Span::raw(part),
                    Span::styled("▌", cursor_style),
                ]));
            } else if i == 0 {
                lines.push(Line::from(vec![
                    Span::styled(format!("{} ", ts), ts_style),
                    Span::styled("[Agent]  ", prefix_style),
                    Span::raw(part),
                ]));
            } else if i == last {
                lines.push(Line::from(vec![
                    Span::raw(indent.clone()),
                    Span::raw(part),
                    Span::styled("▌", cursor_style),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::raw(indent.clone()),
                    Span::raw(part),
                ]));
            }
        }
        items.push(ListItem::new(Text::from(lines)));
    }

    if let Some(ref approval) = state.pending_approval {
        let line = Line::from(vec![
            Span::styled(
                format!("⚠ Approve [{}]? (y=yes / n=no): {}", approval.tool_name, approval.description),
                Style::default().fg(theme.approval_fg).bg(theme.approval_bg).add_modifier(Modifier::BOLD),
            ),
        ]);
        items.push(ListItem::new(line));
    }

    let block = Block::default()
        .title(" Conversation [1] ")
        .borders(Borders::ALL)
        .border_style(bstyle);

    let list = List::new(items).block(block);
    frame.render_widget(list, msg_area);

    let input_text = format!("> {}█", input_buffer);
    let input_widget = Paragraph::new(input_text)
        .block(Block::default().title(" Input ").borders(Borders::ALL).border_style(bstyle))
        .style(Style::default().fg(theme.text_primary));
    frame.render_widget(input_widget, input_area);
}


