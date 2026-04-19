use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::layout::border_style;
use crate::state::{AppState, MessageRole, PaneId};

pub fn render(frame: &mut Frame, state: &AppState, area: Rect, input_buffer: &str) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(area);

    let msg_area = chunks[0];
    let input_area = chunks[1];

    let bstyle = border_style(&PaneId::Conversation, &state.focused_pane);
    let mut items: Vec<ListItem> = state.messages.iter().map(|m| {
        let (prefix, color) = match m.role {
            MessageRole::User => ("[You]    ", Color::Green),
            MessageRole::Assistant => ("[Agent]  ", Color::Cyan),
            MessageRole::System => ("[System] ", Color::Yellow),
            MessageRole::Tool => ("[Tool]   ", Color::Magenta),
        };
        let ts = m.timestamp.format("%H:%M").to_string();
        let line = Line::from(vec![
            Span::styled(format!("{} ", ts), Style::default().fg(Color::DarkGray)),
            Span::styled(prefix, Style::default().fg(color).add_modifier(Modifier::BOLD)),
            Span::raw(m.content.clone()),
        ]);
        ListItem::new(line)
    }).collect();

    if !state.streaming_chunk.is_empty() {
        let ts = chrono::Utc::now().format("%H:%M").to_string();
        let line = Line::from(vec![
            Span::styled(format!("{} ", ts), Style::default().fg(Color::DarkGray)),
            Span::styled("[Agent]  ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(state.streaming_chunk.clone()),
            Span::styled("▌", Style::default().fg(Color::Cyan).add_modifier(Modifier::SLOW_BLINK)),
        ]);
        items.push(ListItem::new(line));
    }

    if let Some(ref approval) = state.pending_approval {
        let line = Line::from(vec![
            Span::styled(
                format!("⚠ Approve [{}]? (y=yes / n=no): {}", approval.tool_name, approval.description),
                Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD),
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
        .style(Style::default().fg(Color::White));
    frame.render_widget(input_widget, input_area);
}
