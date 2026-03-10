use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, StatefulWidget},
    Frame,
};
use ratatui_image::StatefulImage;

use serde_json::Value;

use crate::app::{hex_to_npub, App};

fn draw_relays(relays: &[Value], frame: &mut Frame, area: Rect) {
    let label = Style::default().fg(Color::DarkGray);
    let mut lines = vec![Line::from(Span::styled("  Relays:", label))];
    for relay in relays {
        let url = relay
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let status = relay
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown");
        let (indicator, color) = match status {
            "Connected" => ("●", Color::Green),
            "Connecting" => ("◌", Color::Yellow),
            _ => ("○", Color::Red),
        };
        lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(format!("{indicator} "), Style::default().fg(color)),
            Span::styled(url, Style::default().fg(Color::White)),
        ]));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// Format a profile field for display, showing "(not set)" when empty.
fn display<'a>(val: &'a str, fallback: &'a str) -> &'a str {
    if val.is_empty() {
        fallback
    } else {
        val
    }
}

pub fn draw(app: &mut App, frame: &mut Frame, area: Rect) {
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
    let f = |key: &str| -> String {
        profile
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };

    let has_image = app.profile_image.is_some();
    let info_height = 9;
    let relay_height = if app.account_relays.is_empty() {
        0
    } else {
        app.account_relays.len() as u16 + 2 // header + relays + blank line
    };

    let vertical = Layout::vertical([
        Constraint::Length(info_height), // Profile info (with optional image)
        Constraint::Length(relay_height), // Relays (0 if empty)
        Constraint::Length(1),           // Follows header
        Constraint::Fill(1),             // Follows list
        Constraint::Length(2),           // Hints (2 rows)
    ])
    .split(inner);

    // Profile info section: image (left) + text fields (right)
    let name = f("name");
    let display_name = f("display_name");
    let about = f("about");
    let picture = f("picture");
    let nip05 = f("nip05");
    let lud16 = f("lud16");
    let npub = {
        let raw = f("npub");
        if raw.is_empty() {
            app.account.as_deref().map(hex_to_npub).unwrap_or_default()
        } else if !raw.starts_with("npub") {
            hex_to_npub(&raw)
        } else {
            raw
        }
    };

    let label = Style::default().fg(Color::DarkGray);
    let val = Style::default().fg(Color::White);
    let not_set = "(not set)";

    // Split info area horizontally if we have an image
    let (image_area, text_area) = if has_image && vertical[0].width >= 48 {
        let cols = Layout::horizontal([
            Constraint::Length(20), // Image column
            Constraint::Fill(1),    // Text fields
        ])
        .split(vertical[0]);
        (Some(cols[0]), cols[1])
    } else {
        (None, vertical[0])
    };

    // Render profile image
    if let Some(img_area) = image_area {
        if let Some(protocol) = &mut app.profile_image {
            let image_widget = StatefulImage::default();
            frame.render_stateful_widget(image_widget, img_area, protocol);
        }
    }

    let lines = vec![
        Line::from(vec![
            Span::styled("  Name:          ", label),
            Span::styled(display(&name, not_set), val.add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  Display name:  ", label),
            Span::styled(display(&display_name, not_set), val),
        ]),
        Line::from(vec![
            Span::styled("  About:         ", label),
            Span::styled(display(&about, not_set), val),
        ]),
        Line::from(vec![
            Span::styled("  Picture:       ", label),
            Span::styled(
                display(&picture, not_set),
                Style::default().fg(Color::DarkGray),
            ),
        ]),
        Line::from(vec![
            Span::styled("  NIP-05:        ", label),
            Span::styled(display(&nip05, not_set), val),
        ]),
        Line::from(vec![
            Span::styled("  Lightning:     ", label),
            Span::styled(display(&lud16, not_set), val),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  npub:          ", label),
            Span::styled(npub, val),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines), text_area);

    // Relays
    if !app.account_relays.is_empty() {
        draw_relays(&app.account_relays, frame, vertical[1]);
    }

    // Follows header
    let follows_header = Line::from(vec![Span::styled(
        format!("  Following ({})", app.follows.len()),
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )]);
    frame.render_widget(Paragraph::new(follows_header), vertical[2]);

    // Follows list
    if app.follows.is_empty() {
        let (text, color) = if app.follows_loading {
            ("  Loading follows...", Color::Yellow)
        } else {
            ("  Not following anyone", Color::DarkGray)
        };
        let empty = Paragraph::new(Span::styled(text, Style::default().fg(color)));
        frame.render_widget(empty, vertical[3]);
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
        StatefulWidget::render(list, vertical[3], frame.buffer_mut(), &mut state);
    }

    // Hints (2 rows)
    let hint_rows =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(vertical[4]);

    let line1 = Line::from(vec![
        Span::styled("  [n] ", Style::default().fg(Color::Cyan)),
        Span::raw("Name  "),
        Span::styled("[D] ", Style::default().fg(Color::Cyan)),
        Span::raw("Display name  "),
        Span::styled("[a] ", Style::default().fg(Color::Cyan)),
        Span::raw("About  "),
        Span::styled("[P] ", Style::default().fg(Color::Cyan)),
        Span::raw("Picture  "),
        Span::styled("[5] ", Style::default().fg(Color::Cyan)),
        Span::raw("NIP-05  "),
        Span::styled("[$] ", Style::default().fg(Color::Cyan)),
        Span::raw("Lightning  "),
        Span::styled("[e] ", Style::default().fg(Color::Cyan)),
        Span::raw("Show nsec"),
    ]);
    frame.render_widget(Paragraph::new(line1), hint_rows[0]);

    let mut line2_spans = vec![];
    if !app.follows.is_empty() {
        line2_spans.extend([
            Span::styled("  [j/k] ", Style::default().fg(Color::Cyan)),
            Span::raw("Navigate  "),
            Span::styled("[Enter] ", Style::default().fg(Color::Cyan)),
            Span::raw("View  "),
            Span::styled("[d] ", Style::default().fg(Color::Cyan)),
            Span::raw("Unfollow  "),
        ]);
    } else {
        line2_spans.push(Span::raw("  "));
    }
    line2_spans.extend([
        Span::styled("[Q] ", Style::default().fg(Color::Cyan)),
        Span::raw("Logout  "),
        Span::styled("[Esc] ", Style::default().fg(Color::Cyan)),
        Span::raw("Back"),
    ]);
    frame.render_widget(Paragraph::new(Line::from(line2_spans)), hint_rows[1]);
}
