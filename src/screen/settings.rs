use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::App;

pub fn draw(app: &App, frame: &mut Frame, area: Rect) {
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Settings ");
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    if app.settings_data.is_none() {
        let loading = Paragraph::new(Span::styled(
            "Loading settings...",
            Style::default().fg(Color::Yellow),
        ))
        .centered();
        let centered = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .split(inner);
        frame.render_widget(loading, centered[1]);
        return;
    }

    let settings = app.settings_data.as_ref().unwrap();

    let vertical = Layout::vertical([
        Constraint::Fill(1),   // Settings list
        Constraint::Length(1), // Hints
    ])
    .split(inner);

    // Render settings as key-value pairs
    let mut lines = vec![Line::raw("")];
    if let Some(obj) = settings.as_object() {
        for (key, value) in obj {
            let val_str = match value {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            lines.push(Line::from(vec![
                Span::styled(format!("  {key}: "), Style::default().fg(Color::DarkGray)),
                Span::styled(val_str, Style::default().fg(Color::White)),
            ]));
        }
    } else {
        lines.push(Line::from(Span::styled(
            "  No settings available",
            Style::default().fg(Color::DarkGray),
        )));
    }
    frame.render_widget(Paragraph::new(lines), vertical[0]);

    // Hints
    let hints = Line::from(vec![
        Span::styled("  [Esc] ", Style::default().fg(Color::Cyan)),
        Span::raw("Back"),
    ]);
    frame.render_widget(Paragraph::new(hints), vertical[1]);
}
