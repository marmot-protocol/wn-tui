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
    display_name: Option<&'a str>,
    chat_count: usize,
    unread_total: usize,
    pending_invites: usize,
    connected: bool,
}

impl<'a> StatusBarWidget<'a> {
    pub fn new(account: Option<&'a str>, chat_count: usize) -> Self {
        Self {
            account,
            display_name: None,
            chat_count,
            unread_total: 0,
            pending_invites: 0,
            connected: true,
        }
    }

    pub fn display_name(mut self, name: Option<&'a str>) -> Self {
        self.display_name = name;
        self
    }

    pub fn unread_total(mut self, count: usize) -> Self {
        self.unread_total = count;
        self
    }

    pub fn pending_invites(mut self, count: usize) -> Self {
        self.pending_invites = count;
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

        let npub_display = self.account.map_or_else(
            || "not logged in".to_string(),
            |a| {
                let npub = crate::app::hex_to_npub(a);
                if npub.len() > 20 {
                    format!("{}...{}", &npub[..12], &npub[npub.len() - 6..])
                } else {
                    npub
                }
            },
        );

        let bar_bg = Style::default().fg(Color::White).bg(Color::DarkGray);
        let sep = Span::styled(" │ ", Style::default().fg(Color::Gray).bg(Color::DarkGray));

        let mut spans = vec![
            Span::raw(" "),
            Span::styled(status_icon, Style::default().fg(status_color)),
        ];

        // Display name (if available) followed by npub
        if let Some(name) = self.display_name.filter(|n| !n.is_empty()) {
            spans.push(Span::styled(format!(" {name}"), bar_bg));
            spans.push(Span::styled(
                format!(" {npub_display}"),
                Style::default().fg(Color::Gray).bg(Color::DarkGray),
            ));
        } else {
            spans.push(Span::styled(format!(" {npub_display}"), bar_bg));
        }

        spans.push(sep.clone());
        spans.push(Span::styled(format!("{} chats", self.chat_count), bar_bg));

        if self.pending_invites > 0 {
            spans.push(sep.clone());
            spans.push(Span::styled(
                format!("{} pending invites", self.pending_invites),
                Style::default().fg(Color::Magenta).bg(Color::DarkGray),
            ));
        }

        if self.unread_total > 0 {
            spans.push(sep);
            spans.push(Span::styled(
                format!("{} unread", self.unread_total),
                Style::default().fg(Color::Yellow).bg(Color::DarkGray),
            ));
        }

        Paragraph::new(Line::from(spans)).render(area, buf);
    }
}
