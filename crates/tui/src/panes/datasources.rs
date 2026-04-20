use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

use crate::{layout, state::{AppState, DataSourceActivity, PaneId}};

pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    let theme = &state.theme;
    let block = Block::default()
        .title(" Data Sources [7] ")
        .borders(Borders::ALL)
        .border_style(layout::border_style(&PaneId::DataSources, &state.focused_pane, theme));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if state.data_sources.is_empty() {
        let empty = List::new(vec![ListItem::new(
            Line::from(Span::styled("no activity yet", Style::default().fg(theme.text_dim))),
        )]);
        f.render_widget(empty, inner);
        return;
    }

    let max_items = inner.height as usize;
    let items: Vec<ListItem> = state
        .data_sources
        .iter()
        .rev()
        .take(max_items)
        .map(|a| render_activity(a, state))
        .collect();

    let list = List::new(items);
    f.render_widget(list, inner);
}

fn source_color(source: &str, state: &AppState) -> Color {
    let theme = &state.theme;
    match source {
        "obsidian"      => theme.ds_obsidian,
        "qdrant"        => theme.ds_qdrant,
        "postgres"      => theme.ds_postgres,
        "context_files" => theme.ds_other,
        _               => theme.ds_other,
    }
}

fn render_activity(a: &DataSourceActivity, state: &AppState) -> ListItem<'static> {
    let theme = &state.theme;
    let time = a.timestamp.format("%H:%M:%S").to_string();
    let badge = format!("[{}]", a.source.to_uppercase());
    let detail = truncate(&a.detail, 60);

    let line = Line::from(vec![
        Span::styled(time, Style::default().fg(theme.text_timestamp)),
        Span::raw(" "),
        Span::styled(
            badge,
            Style::default()
                .fg(source_color(&a.source, state))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(detail, Style::default().fg(theme.text_primary)),
    ]);
    ListItem::new(line)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}
