use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::App;
use crate::widget::input::InputWidget;

/// What the login screen is showing.
#[derive(Debug, Clone, PartialEq)]
pub enum LoginMode {
    Menu,
    NsecInput,
    Loading(String),
    AccountSelect {
        accounts: Vec<serde_json::Value>,
        selected: usize,
    },
}

pub fn draw(app: &App, frame: &mut Frame, area: Rect) {
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" wn-tui ");

    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let content_height = match &app.login_mode {
        LoginMode::AccountSelect { accounts, .. } => (accounts.len() as u16 + 5).min(inner.height),
        _ => 7,
    };
    let vertical = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(content_height),
        Constraint::Fill(1),
    ])
    .split(inner);

    let content_area = vertical[1];
    let horizontal = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(50.min(content_area.width)),
        Constraint::Fill(1),
    ])
    .split(content_area);

    let center = horizontal[1];

    match &app.login_mode {
        LoginMode::Menu => draw_menu(app, frame, center),
        LoginMode::NsecInput => draw_nsec_input(app, frame, center),
        LoginMode::Loading(msg) => draw_loading(msg, frame, center),
        LoginMode::AccountSelect { accounts, selected } => {
            draw_account_select(accounts, *selected, frame, center)
        }
    }
}

fn draw_menu(app: &App, frame: &mut Frame, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(1), // Title
        Constraint::Length(1), // Spacer
        Constraint::Length(1), // Option 1
        Constraint::Length(1), // Option 2
        Constraint::Length(1), // Spacer
        Constraint::Length(1), // Status
    ])
    .split(area);

    let title = Paragraph::new(Line::from(vec![
        Span::styled(
            "White",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "Noise",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ]))
    .centered();
    frame.render_widget(title, rows[0]);

    let opt1 = Paragraph::new(Line::from(vec![
        Span::styled("  [c] ", Style::default().fg(Color::Cyan)),
        Span::raw("Create new identity"),
    ]))
    .centered();
    frame.render_widget(opt1, rows[2]);

    let opt2 = Paragraph::new(Line::from(vec![
        Span::styled("  [l] ", Style::default().fg(Color::Cyan)),
        Span::raw("Login with nsec"),
    ]))
    .centered();
    frame.render_widget(opt2, rows[3]);

    if let Some(msg) = &app.status_message {
        let style = if msg.starts_with("Error") {
            Style::default().fg(Color::Red)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let status = Paragraph::new(Span::styled(msg.as_str(), style)).centered();
        frame.render_widget(status, rows[5]);
    } else {
        let quit_hint = Paragraph::new(Line::from(vec![
            Span::styled("  [q] ", Style::default().fg(Color::DarkGray)),
            Span::styled("Quit", Style::default().fg(Color::DarkGray)),
        ]))
        .centered();
        frame.render_widget(quit_hint, rows[5]);
    }
}

fn draw_nsec_input(app: &App, frame: &mut Frame, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(1), // Title
        Constraint::Length(1), // Spacer
        Constraint::Length(3), // Input
        Constraint::Length(1), // Spacer
        Constraint::Length(1), // Hint
    ])
    .split(area);

    let title = Paragraph::new(Span::styled(
        "Enter your nsec",
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    ))
    .centered();
    frame.render_widget(title, rows[0]);

    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" nsec ");

    let input_widget = InputWidget::new(&app.nsec_input)
        .focused(true)
        .block(input_block);
    frame.render_widget(input_widget, rows[2]);

    let hint = Paragraph::new(Line::from(vec![
        Span::styled("[Enter] ", Style::default().fg(Color::Cyan)),
        Span::raw("Submit   "),
        Span::styled("[Esc] ", Style::default().fg(Color::Cyan)),
        Span::raw("Back"),
    ]))
    .centered();
    frame.render_widget(hint, rows[4]);
}

fn draw_account_select(
    accounts: &[serde_json::Value],
    selected: usize,
    frame: &mut Frame,
    area: Rect,
) {
    let mut constraints = vec![
        Constraint::Length(1), // Title
        Constraint::Length(1), // Spacer
    ];
    for _ in accounts {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Length(1)); // Spacer
    constraints.push(Constraint::Length(1)); // Hints

    let rows = Layout::vertical(constraints).split(area);

    let title = Paragraph::new(Line::from(vec![Span::styled(
        "Select Account",
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )]))
    .centered();
    frame.render_widget(title, rows[0]);

    for (i, account) in accounts.iter().enumerate() {
        let pubkey = account
            .get("pubkey")
            .or_else(|| account.get("npub"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let short_pubkey = if pubkey.len() > 20 {
            format!("{}...{}", &pubkey[..8], &pubkey[pubkey.len() - 8..])
        } else {
            pubkey.to_string()
        };
        let display_name = account
            .get("display_name")
            .or_else(|| account.get("name"))
            .and_then(|v| v.as_str());
        let marker = if i == selected { ">" } else { " " };
        let style = if i == selected {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        let mut spans = vec![Span::styled(
            format!("  {marker} "),
            Style::default().fg(Color::Cyan),
        )];
        if let Some(name) = display_name {
            spans.push(Span::styled(name.to_string(), style));
            spans.push(Span::styled(
                format!("  {short_pubkey}"),
                Style::default().fg(Color::DarkGray),
            ));
        } else {
            spans.push(Span::styled(short_pubkey, style));
        }
        let line = Paragraph::new(Line::from(spans));
        frame.render_widget(line, rows[2 + i]);
    }

    let hint_row = rows.len() - 1;
    let hint = Paragraph::new(Line::from(vec![
        Span::styled("[j/k] ", Style::default().fg(Color::Cyan)),
        Span::raw("Navigate  "),
        Span::styled("[Enter] ", Style::default().fg(Color::Cyan)),
        Span::raw("Select  "),
        Span::styled("[q] ", Style::default().fg(Color::Cyan)),
        Span::raw("Quit"),
    ]))
    .centered();
    frame.render_widget(hint, rows[hint_row]);
}

fn draw_loading(msg: &str, frame: &mut Frame, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .split(area);

    let loading = Paragraph::new(Span::styled(msg, Style::default().fg(Color::Yellow))).centered();
    frame.render_widget(loading, rows[1]);
}
