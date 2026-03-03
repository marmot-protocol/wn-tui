use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, StatefulWidget, Widget},
    Frame,
};

use crate::app::{App, SearchPurpose};
use crate::widget::input::InputWidget;

pub fn draw(app: &App, frame: &mut Frame, area: Rect) {
    let title = match &app.search_purpose {
        SearchPurpose::AddMember { .. } => " Search User to Add ",
        SearchPurpose::Browse => " User Search ",
    };
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(title);
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let vertical = Layout::vertical([
        Constraint::Length(3), // Search input
        Constraint::Length(1), // Separator / result count
        Constraint::Fill(1),   // Results list
        Constraint::Length(1), // Hints
    ])
    .split(inner);

    // Search input
    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::White))
        .title(" Search ");
    InputWidget::new(&app.search_input)
        .focused(true)
        .block(input_block)
        .render(vertical[0], frame.buffer_mut());

    // Result count
    let count_text = if app.search_results.is_empty() {
        "  Type a query and press Enter to search".to_string()
    } else {
        format!("  {} result(s)", app.search_results.len())
    };
    let count = Paragraph::new(Span::styled(
        count_text,
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(count, vertical[1]);

    // Results list
    if !app.search_results.is_empty() {
        let items: Vec<ListItem> = app
            .search_results
            .iter()
            .enumerate()
            .map(|(i, user)| {
                let metadata = user.get("metadata");
                let name = metadata
                    .and_then(|m| m.get("display_name").or_else(|| m.get("name")))
                    .and_then(|v| v.as_str())
                    // Fall back to top-level fields
                    .or_else(|| {
                        user.get("display_name")
                            .or_else(|| user.get("name"))
                            .and_then(|v| v.as_str())
                    })
                    .unwrap_or("unknown");
                let npub = user
                    .get("pubkey")
                    .or_else(|| user.get("npub"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let short_npub = if npub.len() > 20 {
                    format!("{}...", &npub[..17])
                } else {
                    npub.to_string()
                };

                let marker = if i == app.selected_result { ">" } else { " " };
                let name_style = if i == app.selected_result {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };

                ListItem::new(Line::from(vec![
                    Span::styled(format!("  {marker} "), Style::default().fg(Color::Cyan)),
                    Span::styled(name.to_string(), name_style),
                    Span::styled(
                        format!("  {short_npub}"),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]))
            })
            .collect();

        let list = List::new(items);
        let mut state = ListState::default();
        state.select(Some(app.selected_result));
        StatefulWidget::render(list, vertical[2], frame.buffer_mut(), &mut state);
    }

    // Hints
    let select_hint = match &app.search_purpose {
        SearchPurpose::AddMember { .. } if !app.search_results.is_empty() => "Add to group  ",
        _ => "",
    };
    let mut hint_spans = vec![Span::styled("  [Enter] ", Style::default().fg(Color::Cyan))];
    if !select_hint.is_empty() {
        hint_spans.push(Span::raw(select_hint));
    } else {
        hint_spans.push(Span::raw("Search  "));
    }
    hint_spans.extend([
        Span::styled("[↑/↓] ", Style::default().fg(Color::Cyan)),
        Span::raw("Navigate  "),
        Span::styled("[Esc] ", Style::default().fg(Color::Cyan)),
        Span::raw("Back"),
    ]);
    frame.render_widget(Paragraph::new(Line::from(hint_spans)), vertical[3]);
}
