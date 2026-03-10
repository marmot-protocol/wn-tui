use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use serde_json::Value;

use crate::app::{App, HealthView};

// ── Data extraction ──────────────────────────────────────────────────

struct RelayEntry {
    url: String,
    status: String,
}

fn status_order(status: &str) -> u8 {
    match status {
        "Disconnected" => 0,
        "Pending" => 1,
        "Connecting" => 2,
        "Connected" => 3,
        _ => 4,
    }
}

fn status_color(status: &str) -> Color {
    match status {
        "Connected" => Color::Green,
        "Connecting" => Color::Yellow,
        "Pending" => Color::Yellow,
        "Disconnected" => Color::Red,
        _ => Color::DarkGray,
    }
}

fn status_icon(status: &str) -> &'static str {
    match status {
        "Connected" => "●",
        "Connecting" => "◐",
        "Pending" => "○",
        "Disconnected" => "✕",
        _ => "?",
    }
}

fn collect_from_session(
    session: &Value,
    section: &'static str,
    relays: &mut Vec<(String, String, &'static str)>,
) {
    let Some(arr) = session.get("relays").and_then(|v| v.as_array()) else {
        return;
    };
    for relay in arr {
        let url = relay
            .get("relay_url")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let status = relay
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown");
        relays.push((url.to_string(), status.to_string(), section));
    }
}

/// Extract all relays, deduplicating by URL, tracking sections, keeping worst status.
fn extract_relays(val: &Value) -> Vec<RelayEntry> {
    let mut raw: Vec<(String, String, &'static str)> = Vec::new();

    if let Some(accounts) = val
        .get("account_inbox")
        .and_then(|v| v.get("accounts"))
        .and_then(|v| v.as_array())
    {
        for acct in accounts {
            if let Some(session) = acct.get("session") {
                collect_from_session(session, "inbox", &mut raw);
            }
        }
    }

    if let Some(session) = val.get("discovery").and_then(|v| v.get("session")) {
        collect_from_session(session, "discovery", &mut raw);
    }

    if let Some(session) = val.get("group").and_then(|v| v.get("session")) {
        collect_from_session(session, "group", &mut raw);
    }

    if let Some(eph) = val.get("ephemeral") {
        if let Some(accounts) = eph.get("accounts").and_then(|v| v.as_array()) {
            for acct in accounts {
                if let Some(session) = acct.get("session") {
                    collect_from_session(session, "ephemeral", &mut raw);
                }
            }
        }
        if let Some(session) = eph.get("anonymous").and_then(|v| v.get("session")) {
            collect_from_session(session, "ephemeral", &mut raw);
        }
    }

    let mut map: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for (url, status, _section) in raw {
        let entry = map.entry(url).or_insert_with(|| status.clone());
        // Keep the worst (lowest order) status
        if status_order(&status) < status_order(entry) {
            *entry = status;
        }
    }

    let mut relays: Vec<RelayEntry> = map
        .into_iter()
        .map(|(url, status)| RelayEntry { url, status })
        .collect();

    relays.sort_by(|a, b| {
        status_order(&a.status)
            .cmp(&status_order(&b.status))
            .then(a.url.cmp(&b.url))
    });

    relays
}

/// Extract relays for a single plane, preserving per-relay status.
fn extract_plane_relays(val: &Value, plane: &str) -> Vec<(String, String)> {
    let mut relays = Vec::new();
    let collect = |session: &Value, out: &mut Vec<(String, String)>| {
        if let Some(arr) = session.get("relays").and_then(|v| v.as_array()) {
            for r in arr {
                let url = r.get("relay_url").and_then(|v| v.as_str()).unwrap_or("?");
                let status = r.get("status").and_then(|v| v.as_str()).unwrap_or("?");
                match out.iter_mut().find(|(u, _)| u == url) {
                    Some((_, existing)) if status_order(status) < status_order(existing) => {
                        *existing = status.to_string();
                    }
                    Some(_) => {}
                    None => out.push((url.to_string(), status.to_string())),
                }
            }
        }
    };

    match plane {
        "inbox" => {
            if let Some(accounts) = val
                .get("account_inbox")
                .and_then(|v| v.get("accounts"))
                .and_then(|v| v.as_array())
            {
                for acct in accounts {
                    if let Some(session) = acct.get("session") {
                        collect(session, &mut relays);
                    }
                }
            }
        }
        "discovery" => {
            if let Some(session) = val.get("discovery").and_then(|v| v.get("session")) {
                collect(session, &mut relays);
            }
        }
        "group" => {
            if let Some(session) = val.get("group").and_then(|v| v.get("session")) {
                collect(session, &mut relays);
            }
        }
        "ephemeral" => {
            if let Some(eph) = val.get("ephemeral") {
                if let Some(accounts) = eph.get("accounts").and_then(|v| v.as_array()) {
                    for acct in accounts {
                        if let Some(session) = acct.get("session") {
                            collect(session, &mut relays);
                        }
                    }
                }
                if let Some(session) = eph.get("anonymous").and_then(|v| v.get("session")) {
                    collect(session, &mut relays);
                }
            }
        }
        _ => {}
    }

    relays.sort_by(|a, b| {
        status_order(&a.1)
            .cmp(&status_order(&b.1))
            .then(a.0.cmp(&b.0))
    });
    relays
}

fn get_u64(val: &Value, path: &[&str]) -> u64 {
    let mut v = val;
    for key in path {
        match v.get(key) {
            Some(next) => v = next,
            None => return 0,
        }
    }
    v.as_u64().unwrap_or(0)
}

// ── Rendering helpers ────────────────────────────────────────────────

fn status_counts_line(relays: &[RelayEntry]) -> Line<'static> {
    let count = |s: &str| relays.iter().filter(|r| r.status == s).count();
    let connected = count("Connected");
    let connecting = count("Connecting");
    let pending = count("Pending");
    let disconnected = count("Disconnected");

    let mut spans = vec![Span::raw("  ")];
    if connected > 0 {
        spans.push(Span::styled(
            format!("● {connected} connected"),
            Style::default().fg(Color::Green),
        ));
        spans.push(Span::raw("  "));
    }
    if connecting > 0 {
        spans.push(Span::styled(
            format!("◐ {connecting} connecting"),
            Style::default().fg(Color::Yellow),
        ));
        spans.push(Span::raw("  "));
    }
    if pending > 0 {
        spans.push(Span::styled(
            format!("○ {pending} pending"),
            Style::default().fg(Color::Yellow),
        ));
        spans.push(Span::raw("  "));
    }
    if disconnected > 0 {
        spans.push(Span::styled(
            format!("✕ {disconnected} disconnected"),
            Style::default().fg(Color::Red),
        ));
    }
    Line::from(spans)
}

fn summary_line(val: &Value, relays: &[RelayEntry]) -> Line<'static> {
    let accounts = get_u64(val, &["account_inbox", "active_account_count"]);
    let groups = get_u64(val, &["group", "group_count"]);
    let watched = get_u64(val, &["discovery", "watched_user_count"]);
    let connected = relays.iter().filter(|r| r.status == "Connected").count();
    let total = relays.len();
    let dim = Style::default().fg(Color::DarkGray);

    Line::from(vec![
        Span::styled(format!("  {connected}/{total} relays connected"), dim),
        Span::styled("  |  ", dim),
        Span::styled(format!("{accounts} accounts"), dim),
        Span::styled("  ", dim),
        Span::styled(format!("{groups} groups"), dim),
        Span::styled("  ", dim),
        Span::styled(format!("{watched} watched users"), dim),
    ])
}

fn section_header(label: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("  {label}"),
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    ))
}

/// Lay out relay entries in newspaper-style columns (fill down, then across).
/// Each entry shows: icon + URL, padded to uniform width per column.
fn columnize_relays(
    relays: &[(impl AsRef<str>, impl AsRef<str>)],
    width: u16,
) -> Vec<Line<'static>> {
    if relays.is_empty() {
        return vec![];
    }

    let max_url = relays
        .iter()
        .map(|(u, _)| u.as_ref().len())
        .max()
        .unwrap_or(20);
    // Each cell: indent(4) + icon(2) + url(max_url) + gap(2)
    let col_width = max_url + 8;
    let num_cols = ((width as usize) / col_width).max(1);
    let num_rows = relays.len().div_ceil(num_cols);

    let mut lines = Vec::with_capacity(num_rows);
    for row in 0..num_rows {
        let mut spans: Vec<Span> = Vec::new();
        for col in 0..num_cols {
            let idx = col * num_rows + row;
            if idx < relays.len() {
                let url = relays[idx].0.as_ref();
                let status = relays[idx].1.as_ref();
                let icon = status_icon(status);
                let color = status_color(status);
                spans.push(Span::styled(
                    format!("    {icon} "),
                    Style::default().fg(color),
                ));
                spans.push(Span::styled(
                    format!("{:<width$}", url, width = max_url + 2),
                    Style::default().fg(Color::White),
                ));
            }
        }
        lines.push(Line::from(spans));
    }
    lines
}

// ── View renderers ───────────────────────────────────────────────────

fn build_by_status(relays: &[RelayEntry], width: u16) -> Vec<Line<'static>> {
    let mut lines: Vec<Line> = Vec::new();

    // Group relays by status
    let mut current_status: Option<&str> = None;
    let mut section_batch: Vec<(&str, &str)> = Vec::new();

    for relay in relays {
        if current_status != Some(&relay.status) {
            // Flush previous batch
            if !section_batch.is_empty() {
                lines.extend(columnize_relays(&section_batch, width));
                section_batch.clear();
            }
            if current_status.is_some() {
                lines.push(Line::raw(""));
            }
            lines.push(section_header(&relay.status));
            current_status = Some(&relay.status);
        }
        section_batch.push((&relay.url, &relay.status));
    }
    // Flush last batch
    if !section_batch.is_empty() {
        lines.extend(columnize_relays(&section_batch, width));
    }

    lines
}

fn build_by_plane(val: &Value, width: u16) -> Vec<Line<'static>> {
    let dim = Style::default().fg(Color::DarkGray);
    let mut lines: Vec<Line> = Vec::new();

    // ── Inbox
    let inbox_relays = extract_plane_relays(val, "inbox");
    let inbox_accounts = get_u64(val, &["account_inbox", "active_account_count"]);
    let inbox_subs: u64 = val
        .get("account_inbox")
        .and_then(|v| v.get("accounts"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|a| {
                    a.get("session")
                        .and_then(|s| s.get("registered_subscription_count"))
                        .and_then(|v| v.as_u64())
                })
                .sum()
        })
        .unwrap_or(0);
    let inbox_connected = inbox_relays
        .iter()
        .filter(|(_, s)| s == "Connected")
        .count();
    lines.push(section_header("Account Inbox"));
    lines.push(Line::from(Span::styled(
        format!(
            "    {inbox_accounts} accounts, {}/{} relays, {inbox_subs} subscriptions",
            inbox_connected,
            inbox_relays.len()
        ),
        dim,
    )));
    lines.extend(columnize_relays(&inbox_relays, width));

    // ── Discovery
    lines.push(Line::raw(""));
    let disc_relays = extract_plane_relays(val, "discovery");
    let disc_subs = get_u64(
        val,
        &["discovery", "session", "registered_subscription_count"],
    );
    let disc_watched = get_u64(val, &["discovery", "watched_user_count"]);
    let disc_follows = get_u64(val, &["discovery", "follow_list_subscription_count"]);
    let disc_connected = disc_relays.iter().filter(|(_, s)| s == "Connected").count();
    lines.push(section_header("Discovery"));
    lines.push(Line::from(Span::styled(
        format!(
            "    {}/{} relays, {disc_subs} subs, {disc_follows} follow lists, {disc_watched} watched users",
            disc_connected,
            disc_relays.len()
        ),
        dim,
    )));
    lines.extend(columnize_relays(&disc_relays, width));

    // ── Group
    lines.push(Line::raw(""));
    let group_relays = extract_plane_relays(val, "group");
    let group_count = get_u64(val, &["group", "group_count"]);
    let group_subs = get_u64(val, &["group", "session", "registered_subscription_count"]);
    let group_contexts = get_u64(val, &["group", "session", "router_context_count"]);
    let group_connected = group_relays
        .iter()
        .filter(|(_, s)| s == "Connected")
        .count();
    lines.push(section_header("Group"));
    lines.push(Line::from(Span::styled(
        format!(
            "    {group_count} groups, {}/{} relays, {group_subs} subs, {group_contexts} router contexts",
            group_connected,
            group_relays.len()
        ),
        dim,
    )));
    lines.extend(columnize_relays(&group_relays, width));

    // ── Ephemeral
    lines.push(Line::raw(""));
    let eph_relays = extract_plane_relays(val, "ephemeral");
    let eph_scopes = get_u64(val, &["ephemeral", "account_scope_count"]);
    let eph_ad_hoc = val
        .get("ephemeral")
        .and_then(|v| v.get("anonymous"))
        .and_then(|v| v.get("ad_hoc_relay_count"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let eph_pinned = val
        .get("ephemeral")
        .and_then(|v| v.get("anonymous"))
        .and_then(|v| v.get("pinned_relay_count"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let eph_connected = eph_relays.iter().filter(|(_, s)| s == "Connected").count();
    lines.push(section_header("Ephemeral"));
    lines.push(Line::from(Span::styled(
        format!(
            "    {eph_scopes} scopes, {}/{} relays ({eph_pinned} pinned, {eph_ad_hoc} ad-hoc)",
            eph_connected,
            eph_relays.len()
        ),
        dim,
    )));
    lines.extend(columnize_relays(&eph_relays, width));

    lines
}

// ── Main draw ────────────────────────────────────────────────────────

pub fn draw(app: &mut App, frame: &mut Frame, area: Rect) {
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Relay Health ");
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    if app.relay_health.is_none() {
        let (text, color) = match &app.health_error {
            Some(err) => (format!("Failed: {err}  (r to retry)"), Color::Red),
            None => ("Loading relay state...".into(), Color::Yellow),
        };
        let msg = Paragraph::new(Span::styled(text, Style::default().fg(color))).centered();
        let centered = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .split(inner);
        frame.render_widget(msg, centered[1]);
        return;
    }

    let val = app.relay_health.as_ref().unwrap();
    let relays = extract_relays(val);

    let vertical = Layout::vertical([
        Constraint::Length(3), // Header
        Constraint::Fill(1),   // Content
        Constraint::Length(1), // Hints
    ])
    .split(inner);

    // Header
    let header = vec![
        status_counts_line(&relays),
        summary_line(val, &relays),
        Line::raw(""),
    ];
    frame.render_widget(Paragraph::new(header), vertical[0]);

    // Content (width-aware for multi-column)
    let content_width = vertical[1].width;
    let content_lines = match app.health_view {
        HealthView::ByStatus => build_by_status(&relays, content_width),
        HealthView::ByPlane => build_by_plane(val, content_width),
    };
    let viewport_height = vertical[1].height as usize;
    app.health_max_scroll = content_lines.len().saturating_sub(viewport_height);
    app.health_scroll = app.health_scroll.min(app.health_max_scroll);
    let scroll = app.health_scroll as u16;
    frame.render_widget(
        Paragraph::new(content_lines).scroll((scroll, 0)),
        vertical[1],
    );

    // Hints
    let (active_label, inactive_label) = match app.health_view {
        HealthView::ByStatus => ("By Status", "By Plane"),
        HealthView::ByPlane => ("By Plane", "By Status"),
    };
    let hints = Line::from(vec![
        Span::styled("  [Tab] ", Style::default().fg(Color::Cyan)),
        Span::styled(
            format!(" {active_label} "),
            Style::default().fg(Color::Black).bg(Color::DarkGray),
        ),
        Span::styled(
            format!(" {inactive_label}"),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw("    "),
        Span::styled("[r] ", Style::default().fg(Color::Cyan)),
        Span::raw("Refresh  "),
        Span::styled("[j/k] ", Style::default().fg(Color::Cyan)),
        Span::raw("Scroll  "),
        Span::styled("[Esc] ", Style::default().fg(Color::Cyan)),
        Span::raw("Back"),
    ]);
    frame.render_widget(Paragraph::new(hints), vertical[2]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_relays_deduplicates_across_sections() {
        let val = json!({
            "account_inbox": {
                "accounts": [{
                    "session": {
                        "relays": [
                            {"relay_url": "wss://nos.lol", "status": "Connected"},
                            {"relay_url": "wss://relay.damus.io", "status": "Connected"},
                        ]
                    }
                }],
                "active_account_count": 1
            },
            "discovery": {
                "session": {
                    "relays": [
                        {"relay_url": "wss://nos.lol", "status": "Connected"},
                    ]
                },
                "watched_user_count": 10
            },
            "group": {
                "session": {
                    "relays": [
                        {"relay_url": "wss://nos.lol", "status": "Connected"},
                        {"relay_url": "wss://bad.relay", "status": "Disconnected"},
                    ]
                },
                "group_count": 5
            }
        });

        let relays = extract_relays(&val);
        // 3 unique URLs: nos.lol, relay.damus.io, bad.relay
        assert_eq!(relays.len(), 3);
        // Disconnected sorts first
        assert_eq!(relays[0].url, "wss://bad.relay");
        assert_eq!(relays[0].status, "Disconnected");
        // nos.lol appears in all 3 sections but only once in the output
        assert_eq!(
            relays.iter().filter(|r| r.url == "wss://nos.lol").count(),
            1
        );
    }

    #[test]
    fn extract_relays_keeps_worst_status() {
        let val = json!({
            "account_inbox": {
                "accounts": [{
                    "session": {
                        "relays": [
                            {"relay_url": "wss://flaky.relay", "status": "Connected"},
                        ]
                    }
                }],
                "active_account_count": 1
            },
            "group": {
                "session": {
                    "relays": [
                        {"relay_url": "wss://flaky.relay", "status": "Disconnected"},
                    ]
                },
                "group_count": 1
            }
        });

        let relays = extract_relays(&val);
        assert_eq!(relays.len(), 1);
        assert_eq!(relays[0].status, "Disconnected");
    }

    #[test]
    fn extract_relays_empty_state() {
        let relays = extract_relays(&json!({}));
        assert!(relays.is_empty());
    }

    #[test]
    fn extract_plane_relays_returns_section_relays() {
        let val = json!({
            "discovery": {
                "session": {
                    "relays": [
                        {"relay_url": "wss://a.com", "status": "Connected"},
                        {"relay_url": "wss://b.com", "status": "Disconnected"},
                    ]
                }
            }
        });
        let relays = extract_plane_relays(&val, "discovery");
        assert_eq!(relays.len(), 2);
        assert_eq!(relays[0].0, "wss://b.com"); // Disconnected sorts first
    }

    #[test]
    fn columnize_single_column_narrow() {
        let relays = vec![
            ("wss://a.com".to_string(), "Connected".to_string()),
            ("wss://b.com".to_string(), "Disconnected".to_string()),
        ];
        let lines = columnize_relays(&relays, 30);
        assert_eq!(lines.len(), 2); // One per relay, single column
    }

    #[test]
    fn columnize_multiple_columns_wide() {
        let relays: Vec<_> = (0..6)
            .map(|i| {
                (
                    format!("wss://relay{i}.example.com"),
                    "Connected".to_string(),
                )
            })
            .collect();
        // "wss://relay0.example.com" = 24 chars, col_width = 24+8 = 32
        // Width 100: 100/32 = 3 cols, ceil(6/3) = 2 rows
        let lines = columnize_relays(&relays, 100);
        assert_eq!(lines.len(), 2);
        // Width 40: 40/32 = 1 col, 6 rows
        let lines = columnize_relays(&relays, 40);
        assert_eq!(lines.len(), 6);
    }

    #[test]
    fn columnize_empty() {
        let relays: Vec<(String, String)> = vec![];
        let lines = columnize_relays(&relays, 100);
        assert!(lines.is_empty());
    }

    #[test]
    fn build_by_plane_produces_four_sections() {
        let val = json!({
            "account_inbox": {
                "accounts": [{
                    "session": {
                        "registered_subscription_count": 1,
                        "relays": [{"relay_url": "wss://r.com", "status": "Connected"}]
                    }
                }],
                "active_account_count": 1
            },
            "discovery": {
                "session": {
                    "registered_subscription_count": 2,
                    "relays": [{"relay_url": "wss://r.com", "status": "Connected"}]
                },
                "follow_list_subscription_count": 1,
                "watched_user_count": 5
            },
            "group": {
                "session": {
                    "registered_subscription_count": 3,
                    "router_context_count": 10,
                    "relays": [{"relay_url": "wss://r.com", "status": "Connected"}]
                },
                "group_count": 2
            },
            "ephemeral": {
                "account_scope_count": 1,
                "accounts": [],
                "anonymous": {
                    "ad_hoc_relay_count": 5,
                    "pinned_relay_count": 2,
                    "session": {
                        "relays": [{"relay_url": "wss://r.com", "status": "Connected"}]
                    }
                }
            }
        });
        let lines = build_by_plane(&val, 80);
        let text: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        assert!(text.iter().any(|l| l.contains("Account Inbox")));
        assert!(text.iter().any(|l| l.contains("Discovery")));
        assert!(text.iter().any(|l| l.contains("Group")));
        assert!(text.iter().any(|l| l.contains("Ephemeral")));
        assert!(text.iter().any(|l| l.contains("5 watched users")));
    }
}
