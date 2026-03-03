use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::{hex_to_npub, App};

/// Extract a profile field with fallback keys.
fn field(profile: &serde_json::Value, keys: &[&str]) -> String {
    for key in keys {
        if let Some(s) = profile.get(*key).and_then(|v| v.as_str()) {
            return s.to_string();
        }
    }
    String::new()
}

pub fn draw(app: &App, frame: &mut Frame, area: Rect) {
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Profile ");
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    if app.profile.is_none() {
        let loading = Paragraph::new(Span::styled(
            "Loading profile...",
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

    let profile = app.profile.as_ref().unwrap();

    let vertical = Layout::vertical([
        Constraint::Length(7), // Profile info
        Constraint::Fill(1),   // spacer
        Constraint::Length(2), // Hints
    ])
    .split(inner);

    // Profile info
    let name = field(profile, &["name", "display_name"]);
    let about = field(profile, &["about"]);
    let npub = field(profile, &["npub"]);
    let npub = if npub.is_empty() {
        app.account.as_deref().map(hex_to_npub).unwrap_or_default()
    } else if !npub.starts_with("npub") {
        hex_to_npub(&npub)
    } else {
        npub
    };

    let lines = vec![
        Line::raw(""),
        Line::from(vec![
            Span::styled("  Name:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                if name.is_empty() { "(not set)" } else { &name },
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  About:   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                if about.is_empty() {
                    "(not set)"
                } else {
                    &about
                },
                Style::default().fg(Color::White),
            ),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  npub:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(npub, Style::default().fg(Color::White)),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines), vertical[0]);

    // Hints
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(vertical[2]);

    let line1 = Line::from(vec![
        Span::styled("  [n] ", Style::default().fg(Color::Cyan)),
        Span::raw("Edit name  "),
        Span::styled("[a] ", Style::default().fg(Color::Cyan)),
        Span::raw("Edit about  "),
    ]);
    frame.render_widget(Paragraph::new(line1), rows[0]);

    let line2 = Line::from(vec![
        Span::styled("  [Esc] ", Style::default().fg(Color::Cyan)),
        Span::raw("Back"),
    ]);
    frame.render_widget(Paragraph::new(line2), rows[1]);
}
