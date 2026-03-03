use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, StatefulWidget},
    Frame,
};
use serde_json::Value;

use crate::app::App;

/// Extract a string field from a JSON value with fallback.
fn field(val: &Value, keys: &[&str]) -> String {
    for key in keys {
        if let Some(s) = val.get(*key).and_then(|v| v.as_str()) {
            return s.to_string();
        }
    }
    String::new()
}

/// Check if a member npub is in the admins list.
fn is_admin(npub: &str, admins: &[Value]) -> bool {
    admins.iter().any(|a| {
        a.as_str() == Some(npub)
            || a.get("npub").and_then(|v| v.as_str()) == Some(npub)
            || a.get("pubkey").and_then(|v| v.as_str()) == Some(npub)
    })
}

/// Extract npub from a member value.
pub fn member_npub(member: &Value) -> Option<&str> {
    member
        .get("npub")
        .or_else(|| member.get("pubkey"))
        .and_then(|v| v.as_str())
        .or_else(|| member.as_str())
}

/// Extract display name from a member value.
fn member_name(member: &Value) -> String {
    if let Some(name) = member
        .get("display_name")
        .or_else(|| member.get("name"))
        .and_then(|v| v.as_str())
    {
        return name.to_string();
    }
    // Truncate npub for display
    if let Some(npub) = member_npub(member) {
        if npub.len() > 20 {
            return format!("{}...", &npub[..17]);
        }
        return npub.to_string();
    }
    "unknown".to_string()
}

pub fn draw(app: &App, frame: &mut Frame, area: Rect) {
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let title = if let Some(ref detail) = app.group_detail {
        let name = field(detail, &["name", "group_name"]);
        format!(" Group: {name} ")
    } else {
        " Group Detail ".to_string()
    };

    let outer = outer.title(title);
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    // Loading state
    if app.group_detail.is_none() {
        let loading = Paragraph::new(Span::styled(
            "Loading group details...",
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

    let detail = app.group_detail.as_ref().unwrap();

    // Layout: info section + members list + hints
    let vertical = Layout::vertical([
        Constraint::Length(5), // Group info
        Constraint::Length(1), // Separator
        Constraint::Fill(1),   // Members list
        Constraint::Length(2), // Hints
    ])
    .split(inner);

    // Group info
    draw_info(detail, &app.group_members, frame, vertical[0]);

    // Separator
    let sep = Paragraph::new(Span::styled(
        "─── Members ───",
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(sep, vertical[1]);

    // Members list
    draw_members(
        &app.group_members,
        &app.group_admins,
        app.selected_member,
        frame,
        vertical[2],
    );

    // Hints
    draw_hints(frame, vertical[3]);
}

fn draw_info(detail: &Value, members: &[Value], frame: &mut Frame, area: Rect) {
    let name = field(detail, &["name", "group_name"]);
    let description = field(detail, &["description"]);
    let member_count = if members.is_empty() {
        detail
            .get("member_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize
    } else {
        members.len()
    };

    let lines = vec![
        Line::from(vec![
            Span::styled("  Name:        ", Style::default().fg(Color::DarkGray)),
            Span::styled(name, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  Description: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                if description.is_empty() {
                    "(none)".to_string()
                } else {
                    description
                },
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Members:     ", Style::default().fg(Color::DarkGray)),
            Span::styled(member_count.to_string(), Style::default().fg(Color::White)),
        ]),
    ];

    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_members(
    members: &[Value],
    admins: &[Value],
    selected: usize,
    frame: &mut Frame,
    area: Rect,
) {
    if members.is_empty() {
        let empty = Paragraph::new(Span::styled(
            "  Loading members...",
            Style::default().fg(Color::DarkGray),
        ));
        frame.render_widget(empty, area);
        return;
    }

    let items: Vec<ListItem> = members
        .iter()
        .enumerate()
        .map(|(i, member)| {
            let name = member_name(member);
            let npub = member_npub(member).unwrap_or("");
            let admin = is_admin(npub, admins);

            let marker = if i == selected { ">" } else { " " };
            let name_style = if i == selected {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let mut spans = vec![
                Span::styled(format!("  {marker} "), Style::default().fg(Color::Cyan)),
                Span::styled(name, name_style),
            ];

            if admin {
                spans.push(Span::styled(
                    " (admin)",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::DIM),
                ));
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items);
    let mut state = ListState::default();
    state.select(Some(selected));
    StatefulWidget::render(list, area, frame.buffer_mut(), &mut state);
}

fn draw_hints(frame: &mut Frame, area: Rect) {
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(area);

    let line1 = Line::from(vec![
        Span::styled("  [j/k] ", Style::default().fg(Color::Cyan)),
        Span::raw("Navigate  "),
        Span::styled("[a] ", Style::default().fg(Color::Cyan)),
        Span::raw("Search & add  "),
        Span::styled("[A] ", Style::default().fg(Color::Cyan)),
        Span::raw("Paste pubkey  "),
        Span::styled("[x] ", Style::default().fg(Color::Cyan)),
        Span::raw("Remove"),
    ]);
    frame.render_widget(Paragraph::new(line1), rows[0]);

    let line2 = Line::from(vec![
        Span::styled("  [R] ", Style::default().fg(Color::Cyan)),
        Span::raw("Rename  "),
        Span::styled("[L] ", Style::default().fg(Color::Cyan)),
        Span::raw("Leave  "),
        Span::styled("[Esc] ", Style::default().fg(Color::Cyan)),
        Span::raw("Back"),
    ]);
    frame.render_widget(Paragraph::new(line2), rows[1]);
}
