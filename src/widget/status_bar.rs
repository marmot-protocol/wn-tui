use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

/// Renders the bottom status bar.
pub struct StatusBarWidget<'a> {
    account: Option<&'a str>,
    chat_count: usize,
    unread_total: usize,
    connected: bool,
}

impl<'a> StatusBarWidget<'a> {
    pub fn new(account: Option<&'a str>, chat_count: usize) -> Self {
        Self {
            account,
            chat_count,
            unread_total: 0,
            connected: true,
        }
    }

    pub fn unread_total(mut self, count: usize) -> Self {
        self.unread_total = count;
        self
    }

    pub fn connected(mut self, connected: bool) -> Self {
        self.connected = connected;
        self
    }
}

impl Widget for StatusBarWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Fill background
        let bg_style = Style::default().bg(Color::DarkGray).fg(Color::White);
        for x in area.x..area.x + area.width {
            buf[(x, area.y)].set_style(bg_style);
        }

        let status_icon = if self.connected { "●" } else { "○" };
        let status_color = if self.connected {
            Color::Green
        } else {
            Color::Red
        };

        let account_display = self.account.map_or_else(
            || "not logged in".to_string(),
            |a| {
                if a.len() > 20 {
                    format!("{}...", &a[..17])
                } else {
                    a.to_string()
                }
            },
        );

        let mut spans = vec![
            Span::raw(" "),
            Span::styled(status_icon, Style::default().fg(status_color)),
            Span::styled(
                format!(" {account_display}"),
                Style::default().fg(Color::White).bg(Color::DarkGray),
            ),
            Span::styled(" │ ", Style::default().fg(Color::Gray).bg(Color::DarkGray)),
            Span::styled(
                format!("{} chats", self.chat_count),
                Style::default().fg(Color::White).bg(Color::DarkGray),
            ),
        ];

        if self.unread_total > 0 {
            spans.push(Span::styled(
                " │ ",
                Style::default().fg(Color::Gray).bg(Color::DarkGray),
            ));
            spans.push(Span::styled(
                format!("{} unread", self.unread_total),
                Style::default().fg(Color::Yellow).bg(Color::DarkGray),
            ));
        }

        Paragraph::new(Line::from(spans)).render(area, buf);
    }
}
