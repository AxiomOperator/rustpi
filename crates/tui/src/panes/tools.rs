use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

use crate::layout::border_style;
use crate::state::{AppState, PaneId, ToolStatus};

pub fn render(frame: &mut Frame, state: &AppState, area: Rect) {
    let theme = &state.theme;
    let bstyle = border_style(&PaneId::Tools, &state.focused_pane, theme);

    let visible: Vec<_> = state.tool_events.iter().rev().take(20).collect();

    let items: Vec<ListItem> = visible.iter().rev().map(|t| {
        let (status_str, color) = match &t.status {
            ToolStatus::Started    => ("[started]  ", theme.tool_started),
            ToolStatus::Stdout(_)  => ("[stdout]   ", theme.text_primary),
            ToolStatus::Stderr(_)  => ("[stderr]   ", theme.tool_stderr),
            ToolStatus::Completed  => ("[done]     ", theme.tool_done),
            ToolStatus::Failed(_)  => ("[failed]   ", theme.tool_failed),
            ToolStatus::Cancelled  => ("[cancelled]", theme.tool_cancelled),
        };
        let extra = match &t.status {
            ToolStatus::Stdout(s) | ToolStatus::Stderr(s) => format!(": {}", s),
            ToolStatus::Failed(r) => format!(": {}", r),
            _ => String::new(),
        };
        let ts = t.timestamp.format("%H:%M:%S").to_string();
        let line = Line::from(vec![
            Span::styled(format!("{} ", ts), Style::default().fg(theme.text_timestamp)),
            Span::styled(status_str, Style::default().fg(color).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {}", t.tool_name), Style::default().fg(theme.text_primary)),
            Span::styled(extra, Style::default().fg(theme.text_dim)),
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
