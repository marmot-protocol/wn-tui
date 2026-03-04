use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, StatefulWidget},
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
        Constraint::Length(1), // Follows header
        Constraint::Fill(1),   // Follows list
        Constraint::Length(1), // Hints
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

    // Follows header
    let follows_header = Line::from(vec![Span::styled(
        format!("  Following ({})", app.follows.len()),
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )]);
    frame.render_widget(Paragraph::new(follows_header), vertical[1]);

    // Follows list
    if app.follows.is_empty() {
        let empty = Paragraph::new(Span::styled(
            "  Not following anyone",
            Style::default().fg(Color::DarkGray),
        ));
        frame.render_widget(empty, vertical[2]);
    } else {
        let items: Vec<ListItem> = app
            .follows
            .iter()
            .enumerate()
            .map(|(i, user)| {
                let name = user
                    .get("metadata")
                    .and_then(|m| m.get("display_name").or_else(|| m.get("name")))
                    .and_then(|v| v.as_str())
                    .or_else(|| {
                        user.get("display_name")
                            .or_else(|| user.get("name"))
                            .and_then(|v| v.as_str())
                    })
                    .unwrap_or("unknown");
                let pk = user.get("pubkey").and_then(|v| v.as_str()).unwrap_or("");
                let short = if pk.len() > 16 {
                    format!("{}...{}", &pk[..8], &pk[pk.len() - 6..])
                } else {
                    pk.to_string()
                };
                let marker = if i == app.selected_follow { ">" } else { " " };
                let style = if i == app.selected_follow {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("  {marker} "), Style::default().fg(Color::Cyan)),
                    Span::styled(name.to_string(), style),
                    Span::styled(format!("  {short}"), Style::default().fg(Color::DarkGray)),
                ]))
            })
            .collect();

        let list = List::new(items);
        let mut state = ListState::default();
        state.select(Some(app.selected_follow));
        StatefulWidget::render(list, vertical[2], frame.buffer_mut(), &mut state);
    }

    // Hints
    let mut hints = vec![
        Span::styled("  [n] ", Style::default().fg(Color::Cyan)),
        Span::raw("Edit name  "),
        Span::styled("[a] ", Style::default().fg(Color::Cyan)),
        Span::raw("Edit about  "),
        Span::styled("[e] ", Style::default().fg(Color::Cyan)),
        Span::raw("Show nsec  "),
    ];
    if !app.follows.is_empty() {
        hints.extend([
            Span::styled("[j/k] ", Style::default().fg(Color::Cyan)),
            Span::raw("Navigate  "),
            Span::styled("[d] ", Style::default().fg(Color::Cyan)),
            Span::raw("Unfollow  "),
        ]);
    }
    hints.extend([
        Span::styled("[Esc] ", Style::default().fg(Color::Cyan)),
        Span::raw("Back"),
    ]);
    frame.render_widget(Paragraph::new(Line::from(hints)), vertical[3]);
}
