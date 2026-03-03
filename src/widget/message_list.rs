use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Widget, Wrap},
};
use serde_json::Value;

/// Extract display name from a message JSON value.
fn author_name(msg: &Value) -> &str {
    msg.get("display_name")
        .or_else(|| msg.get("author_name"))
        .or_else(|| msg.get("author"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
}

/// Extract message content.
fn content(msg: &Value) -> &str {
    msg.get("content").and_then(|v| v.as_str()).unwrap_or("")
}

/// Extract author pubkey.
fn author_pubkey(msg: &Value) -> &str {
    msg.get("author").and_then(|v| v.as_str()).unwrap_or("")
}

/// Extract and format timestamp.
fn timestamp(msg: &Value) -> String {
    if let Some(ts_str) = msg.get("created_at_local").and_then(|v| v.as_str()) {
        // Format: "2026-03-02 22:44:38" — extract HH:MM
        if ts_str.len() >= 16 {
            return ts_str[11..16].to_string();
        }
    }
    if let Some(ts) = msg.get("created_at").and_then(|v| v.as_i64()) {
        if let Some(dt) = chrono::DateTime::from_timestamp(ts, 0) {
            return dt.format("%H:%M").to_string();
        }
    }
    if let Some(ts) = msg.get("created_at").and_then(|v| v.as_str()) {
        return ts.chars().take(5).collect();
    }
    String::new()
}

/// Renders the message list.
/// `scroll_from_bottom` is how many messages to scroll up from the bottom (0 = at bottom).
pub struct MessageListWidget<'a> {
    messages: &'a [Value],
    scroll_from_bottom: usize,
    block: Option<Block<'a>>,
    my_pubkey: Option<&'a str>,
}

impl<'a> MessageListWidget<'a> {
    pub fn new(messages: &'a [Value], scroll_from_bottom: usize) -> Self {
        Self {
            messages,
            scroll_from_bottom,
            block: None,
            my_pubkey: None,
        }
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    pub fn my_pubkey(mut self, pubkey: Option<&'a str>) -> Self {
        self.my_pubkey = pubkey;
        self
    }
}

impl Widget for MessageListWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let inner = if let Some(block) = &self.block {
            let inner = block.inner(area);
            block.clone().render(area, buf);
            inner
        } else {
            area
        };

        if inner.height == 0 || inner.width == 0 || self.messages.is_empty() {
            if self.messages.is_empty() {
                let empty = Paragraph::new(Span::styled(
                    "No messages yet",
                    Style::default().fg(Color::DarkGray),
                ))
                .centered();
                empty.render(inner, buf);
            }
            return;
        }

        let visible_height = inner.height as usize;
        let width = inner.width as usize;

        // Calculate which messages to show
        let total = self.messages.len();
        let end = total.saturating_sub(self.scroll_from_bottom);
        let start = end.saturating_sub(visible_height);
        let visible = &self.messages[start..end];

        for (i, msg) in visible.iter().enumerate() {
            let row = inner.y + i as u16;
            if row >= inner.y + inner.height {
                break;
            }

            let ts = timestamp(msg);
            let author = author_name(msg);
            let text = content(msg);
            let is_mine = self.my_pubkey.is_some_and(|pk| author_pubkey(msg) == pk);

            if is_mine {
                // Right-aligned: content then timestamp
                let formatted = format!("{text}  [{ts}]");
                let padding = width.saturating_sub(formatted.len());
                let line = Line::from(vec![
                    Span::raw(" ".repeat(padding)),
                    Span::styled(text.to_string(), Style::default().fg(Color::Green)),
                    Span::styled(format!("  [{ts}]"), Style::default().fg(Color::DarkGray)),
                ]);
                let area = Rect::new(inner.x, row, inner.width, 1);
                Paragraph::new(line)
                    .wrap(Wrap { trim: false })
                    .render(area, buf);
            } else {
                // Left-aligned: timestamp, author, content
                let line = Line::from(vec![
                    Span::styled(format!("[{ts}] "), Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format!("{author}: "),
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(text.to_string()),
                ]);
                let area = Rect::new(inner.x, row, inner.width, 1);
                Paragraph::new(line)
                    .wrap(Wrap { trim: false })
                    .render(area, buf);
            }
        }
    }
}

/// Calculate the maximum scroll offset for the message list.
#[allow(dead_code)]
pub fn max_scroll(message_count: usize, visible_height: usize) -> usize {
    message_count.saturating_sub(visible_height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_author_display_name() {
        assert_eq!(author_name(&json!({"display_name": "Alice"})), "Alice");
        assert_eq!(author_name(&json!({"author_name": "Bob"})), "Bob");
        assert_eq!(author_name(&json!({"author": "npub1..."})), "npub1...");
        assert_eq!(author_name(&json!({})), "unknown");
    }

    #[test]
    fn extracts_content() {
        assert_eq!(content(&json!({"content": "Hello!"})), "Hello!");
        assert_eq!(content(&json!({})), "");
    }

    #[test]
    fn formats_local_timestamp() {
        let msg = json!({"created_at_local": "2026-03-02 22:44:38"});
        assert_eq!(timestamp(&msg), "22:44");
    }

    #[test]
    fn formats_unix_timestamp() {
        let msg = json!({"created_at": 1709400000});
        let ts = timestamp(&msg);
        assert!(!ts.is_empty());
    }

    #[test]
    fn max_scroll_calculation() {
        assert_eq!(max_scroll(100, 20), 80);
        assert_eq!(max_scroll(5, 20), 0);
        assert_eq!(max_scroll(20, 20), 0);
    }

    #[test]
    fn identifies_own_messages() {
        let my_pk = "abc123";
        let msg = json!({"author": "abc123", "content": "hello"});
        assert_eq!(author_pubkey(&msg), my_pk);
    }
}
