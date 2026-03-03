use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::{App, LogTab, Panel};
use crate::widget::chat_list::ChatListWidget;
use crate::widget::input::InputWidget;
use crate::widget::message_list::MessageListWidget;
use crate::widget::status_bar::StatusBarWidget;

pub fn draw(app: &App, frame: &mut Frame, area: Rect) {
    // Vertical: content [+ log panel] + hints + status bar
    let log_height = if app.show_logs {
        (area.height / 3).max(5) // ~1/3 of screen, minimum 5 rows
    } else {
        0
    };

    let vertical = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(log_height),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    let content_area = vertical[0];
    let log_area = vertical[1];
    let hints_area = vertical[2];
    let status_area = vertical[3];

    // Horizontal: chat list + message panel
    let horizontal = Layout::horizontal([
        Constraint::Length(28.min(content_area.width / 3)),
        Constraint::Fill(1),
    ])
    .split(content_area);

    let chat_list_area = horizontal[0];
    let message_panel_area = horizontal[1];

    draw_chat_list(app, frame, chat_list_area);
    draw_message_panel(app, frame, message_panel_area);
    if app.show_logs {
        draw_log_panel(app, frame, log_area);
    }
    draw_hints(app, frame, hints_area);
    draw_status_bar(app, frame, status_area);
}

fn draw_chat_list(app: &App, frame: &mut Frame, area: Rect) {
    let focused = app.focus == Panel::ChatList;
    let block = Block::default().borders(Borders::ALL).title(" Chats ");

    let widget = ChatListWidget::new(&app.chats, app.selected_chat)
        .focused(focused)
        .unread(&app.unread_counts)
        .block(block);

    frame.render_widget(widget, area);
}

/// Maximum total rows for the composer (including borders).
const MAX_COMPOSER_HEIGHT: u16 = 8;

fn draw_message_panel(app: &App, frame: &mut Frame, area: Rect) {
    // Calculate dynamic composer height based on content.
    // inner_width = total width minus 2 border columns
    let inner_width = area.width.saturating_sub(2);
    let content_lines = app.composer.line_count(inner_width);
    let composer_height = (content_lines + 2).clamp(3, MAX_COMPOSER_HEIGHT); // +2 for borders

    let vertical =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(composer_height)]).split(area);

    let messages_area = vertical[0];
    let composer_area = vertical[1];

    draw_messages(app, frame, messages_area);
    draw_composer(app, frame, composer_area);
}

fn draw_messages(app: &App, frame: &mut Frame, area: Rect) {
    let title = if let Some(ref gid) = app.active_group_id {
        // Try to find the chat name from the selected chat
        let name = app
            .chats
            .get(app.selected_chat)
            .and_then(|c| {
                c.get("name")
                    .or_else(|| c.get("group_name"))
                    .and_then(|v| v.as_str())
            })
            .unwrap_or(gid.as_str());
        format!(" {name} ")
    } else {
        " Messages ".to_string()
    };

    let msg_focused = app.focus == Panel::Messages;
    let border_color = if msg_focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title);

    if app.active_group_id.is_none() {
        // No chat selected — show hint
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let hint = Paragraph::new(Line::from(vec![Span::styled(
            "Select a chat to start messaging",
            Style::default().fg(Color::DarkGray),
        )]))
        .centered();

        let centered = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .split(inner);
        frame.render_widget(hint, centered[1]);
        return;
    }

    let widget = MessageListWidget::new(&app.messages, app.message_scroll)
        .block(block)
        .my_pubkey(app.account.as_deref());
    frame.render_widget(widget, area);
}

fn draw_composer(app: &App, frame: &mut Frame, area: Rect) {
    let focused = app.focus == Panel::Composer;
    let border_color = if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let placeholder = if app.active_group_id.is_some() {
        if focused {
            " Type a message... "
        } else {
            " [i] to compose "
        }
    } else {
        ""
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(placeholder);

    if focused && app.active_group_id.is_some() {
        let widget = InputWidget::new(&app.composer).focused(true).block(block);
        frame.render_widget(widget, area);
    } else {
        frame.render_widget(block, area);
    }
}

fn draw_hints(app: &App, frame: &mut Frame, area: Rect) {
    let key = |k: &str| {
        Span::styled(
            format!(" {k} "),
            Style::default().fg(Color::Black).bg(Color::DarkGray),
        )
    };
    let label = |l: &str| Span::styled(format!(" {l}"), Style::default().fg(Color::DarkGray));
    let sep = || Span::raw("  ");

    let spans: Vec<Span> = match app.focus {
        Panel::ChatList => vec![
            key("j/k"),
            label("Navigate"),
            sep(),
            key("Enter"),
            label("Open"),
            sep(),
            key("n"),
            label("New group"),
            sep(),
            key("g"),
            label("Group info"),
            sep(),
            key("I"),
            label("Invites"),
            sep(),
            key("/"),
            label("Search"),
            sep(),
            key("p"),
            label("Profile"),
            sep(),
            key("S"),
            label("Settings"),
            sep(),
            key("`"),
            label("Logs"),
            sep(),
            key("q"),
            label("Quit"),
        ],
        Panel::Messages => vec![
            key("j/k"),
            label("Scroll"),
            sep(),
            key("G"),
            label("Bottom"),
            sep(),
            key("i"),
            label("Compose"),
            sep(),
            key("Esc"),
            label("Chat list"),
        ],
        Panel::Composer => vec![
            key("Enter"),
            label("Send"),
            sep(),
            key("Esc"),
            label("Back"),
        ],
    };

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_log_panel(app: &App, frame: &mut Frame, area: Rect) {
    let (active_label, inactive_label) = match app.log_tab {
        LogTab::Activity => ("Activity", "Daemon"),
        LogTab::Daemon => ("Daemon", "Activity"),
    };
    let title = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            format!(" {active_label} "),
            Style::default().fg(Color::Black).bg(Color::DarkGray),
        ),
        Span::raw(" "),
        Span::styled(inactive_label, Style::default().fg(Color::DarkGray)),
        Span::styled(
            "  [Tab] switch  [`] close ",
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let log_buf = match app.log_tab {
        LogTab::Activity => &app.logs,
        LogTab::Daemon => &app.daemon_logs,
    };

    let visible_height = inner.height as usize;
    let total = log_buf.len();
    let end = total.saturating_sub(app.log_scroll);
    let start = end.saturating_sub(visible_height);
    let visible = &log_buf[start..end];

    let lines: Vec<Line> = visible
        .iter()
        .map(|entry| {
            Line::from(Span::styled(
                entry.as_str(),
                Style::default().fg(Color::DarkGray),
            ))
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_status_bar(app: &App, frame: &mut Frame, area: Rect) {
    let display_name = app.profile.as_ref().and_then(|p| {
        p.get("name")
            .or_else(|| p.get("display_name"))
            .and_then(|v| v.as_str())
    });
    let widget = StatusBarWidget::new(app.account.as_deref(), app.chats.len())
        .display_name(display_name)
        .unread_total(app.total_unread())
        .pending_invites(app.pending_invites())
        .connected(app.connected);
    frame.render_widget(widget, area);
}
