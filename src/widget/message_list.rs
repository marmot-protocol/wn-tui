use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Widget, Wrap},
};
use serde_json::Value;
use unicode_width::UnicodeWidthStr;

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

/// Estimate how many terminal rows a line of text will occupy at a given width.
fn wrapped_line_count(text_width: usize, available_width: usize) -> usize {
    if available_width == 0 || text_width == 0 {
        return 1;
    }
    text_width.div_ceil(available_width)
}

/// Format reaction summary from `reactions.by_emoji` as a compact line.
/// Returns `None` if no reactions exist.
fn format_reactions(msg: &Value, indent: usize) -> Option<Line<'static>> {
    let by_emoji = msg.get("reactions")?.get("by_emoji")?.as_object()?;
    if by_emoji.is_empty() {
        return None;
    }

    let mut spans = vec![Span::raw(" ".repeat(indent))];
    for (i, (_key, reaction)) in by_emoji.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        let emoji = reaction
            .get("emoji")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let count = reaction.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        spans.push(Span::styled(
            format!("{emoji} {count}"),
            Style::default().fg(Color::Yellow),
        ));
    }
    Some(Line::from(spans))
}

/// Build the display lines for a message.
/// All messages use the same layout: `[HH:MM] author: content`
/// Own messages are distinguished by green author color.
fn format_message(msg: &Value, my_pubkey: Option<&str>) -> Vec<Line<'static>> {
    let ts = timestamp(msg);
    let author = author_name(msg);
    let text = content(msg);
    let is_mine = my_pubkey.is_some_and(|pk| author_pubkey(msg) == pk);

    let author_style = if is_mine {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    };

    let prefix = format!("[{ts}] ");
    let author_prefix = format!("{author}: ");
    let indent = prefix.len() + author_prefix.len();
    let content_lines: Vec<&str> = text.split('\n').collect();

    let mut lines = Vec::new();
    for (i, line_text) in content_lines.iter().enumerate() {
        if i == 0 {
            lines.push(Line::from(vec![
                Span::styled(prefix.clone(), Style::default().fg(Color::DarkGray)),
                Span::styled(author_prefix.clone(), author_style),
                Span::raw(line_text.to_string()),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::raw(" ".repeat(indent)),
                Span::raw(line_text.to_string()),
            ]));
        }
    }

    if let Some(reaction_line) = format_reactions(msg, indent) {
        lines.push(reaction_line);
    }

    lines
}

/// Check whether a message has any reactions.
fn has_reactions(msg: &Value) -> bool {
    msg.get("reactions")
        .and_then(|r| r.get("by_emoji"))
        .and_then(|b| b.as_object())
        .is_some_and(|m| !m.is_empty())
}

/// Estimate the rendered height of a message at a given terminal width.
/// Accounts for explicit newlines, line wrapping, and a reaction line if present.
fn message_height(msg: &Value, width: usize) -> usize {
    let ts = timestamp(msg);
    let author = author_name(msg);
    let text = content(msg);

    let prefix_width = format!("[{ts}] ").width() + format!("{author}: ").width();
    let content_lines: Vec<&str> = text.split('\n').collect();

    let mut total_rows = 0;
    for line_text in &content_lines {
        total_rows += wrapped_line_count(prefix_width + line_text.width(), width);
    }

    if has_reactions(msg) {
        total_rows += 1;
    }

    total_rows.max(1)
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

        // Walk backwards from the end to find which messages fit in the viewport,
        // accounting for wrapped line heights and scroll offset.
        let total = self.messages.len();
        let skip_messages = self.scroll_from_bottom.min(total);

        // Collect messages that fit in the visible area, walking from bottom to top
        let mut visible_msgs: Vec<usize> = Vec::new(); // indices into self.messages
        let mut used_rows = 0;

        let end = total.saturating_sub(skip_messages);
        for i in (0..end).rev() {
            let h = message_height(&self.messages[i], width);
            if used_rows + h > visible_height {
                break;
            }
            used_rows += h;
            visible_msgs.push(i);
        }
        visible_msgs.reverse();

        // Render each visible message
        let mut y = inner.y;
        for &idx in &visible_msgs {
            let msg = &self.messages[idx];
            let lines = format_message(msg, self.my_pubkey);
            let h = message_height(msg, width);

            let msg_area = Rect::new(inner.x, y, inner.width, h as u16);
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .render(msg_area, buf);

            y += h as u16;
            if y >= inner.y + inner.height {
                break;
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

    #[test]
    fn wrapped_line_count_single_line() {
        assert_eq!(wrapped_line_count(10, 80), 1);
        assert_eq!(wrapped_line_count(80, 80), 1);
    }

    #[test]
    fn wrapped_line_count_multi_line() {
        assert_eq!(wrapped_line_count(160, 80), 2);
        assert_eq!(wrapped_line_count(161, 80), 3);
        assert_eq!(wrapped_line_count(240, 80), 3);
    }

    #[test]
    fn wrapped_line_count_edge_cases() {
        assert_eq!(wrapped_line_count(0, 80), 1);
        assert_eq!(wrapped_line_count(10, 0), 1);
    }

    #[test]
    fn message_height_short_message() {
        let msg =
            json!({"content": "hi", "author": "a", "created_at_local": "2026-01-01 10:00:00"});
        // "[10:00] a: hi" is well under 80 chars
        assert_eq!(message_height(&msg, 80), 1);
    }

    #[test]
    fn message_height_long_message() {
        let long_text = "a".repeat(200);
        let msg = json!({"content": long_text, "author": "alice", "created_at_local": "2026-01-01 10:00:00"});
        // "[10:00] alice: " (15 chars) + 200 chars = 215 chars at width 80 = 3 lines
        let h = message_height(&msg, 80);
        assert!(h > 1, "Expected multi-line height, got {h}");
    }

    #[test]
    fn message_height_with_newlines() {
        let msg = json!({
            "content": "line1\nline2\nline3",
            "author": "alice",
            "created_at_local": "2026-01-01 10:00:00"
        });
        let h = message_height(&msg, 80);
        assert_eq!(h, 3, "3 lines of short text should be 3 rows");
    }

    #[test]
    fn message_height_newline_plus_wrapping() {
        let long_line = "x".repeat(100);
        let msg = json!({
            "content": format!("short\n{long_line}"),
            "author": "a",
            "created_at_local": "2026-01-01 10:00:00"
        });
        let h = message_height(&msg, 80);
        // First line: "[10:00] a: short" = 1 row
        // Second line: indent (13) + 100 chars = 113 chars at width 80 = 2 rows
        assert!(h >= 3, "Expected at least 3 rows, got {h}");
    }

    #[test]
    fn format_message_multiline() {
        let msg = json!({
            "content": "hello\nworld",
            "author": "alice",
            "created_at_local": "2026-01-01 10:00:00"
        });
        let lines = format_message(&msg, None);
        assert_eq!(lines.len(), 2, "Should produce 2 Line entries");
    }

    #[test]
    fn format_reactions_empty() {
        let msg =
            json!({"content": "hi", "author": "a", "created_at_local": "2026-01-01 10:00:00"});
        let lines = format_message(&msg, None);
        assert_eq!(lines.len(), 1, "No reactions = no extra line");
    }

    #[test]
    fn format_reactions_single_emoji() {
        let msg = json!({
            "content": "hi",
            "author": "a",
            "created_at_local": "2026-01-01 10:00:00",
            "reactions": {
                "by_emoji": {
                    "👍": { "emoji": "👍", "count": 3 }
                }
            }
        });
        let lines = format_message(&msg, None);
        assert_eq!(lines.len(), 2, "Should have content + reaction line");
        let reaction_text: String = lines[1].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(reaction_text.contains("👍"), "Should contain the emoji");
        assert!(reaction_text.contains("3"), "Should contain the count");
    }

    #[test]
    fn format_reactions_multiple_emojis() {
        let msg = json!({
            "content": "great",
            "author": "a",
            "created_at_local": "2026-01-01 10:00:00",
            "reactions": {
                "by_emoji": {
                    "👍": { "emoji": "👍", "count": 2 },
                    "❤": { "emoji": "❤", "count": 1 },
                    "🎉": { "emoji": "🎉", "count": 5 }
                }
            }
        });
        let lines = format_message(&msg, None);
        assert_eq!(lines.len(), 2);
        let reaction_text: String = lines[1].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(reaction_text.contains("👍"));
        assert!(reaction_text.contains("❤"));
        assert!(reaction_text.contains("🎉"));
    }

    #[test]
    fn message_height_includes_reactions() {
        let msg = json!({
            "content": "hi",
            "author": "a",
            "created_at_local": "2026-01-01 10:00:00",
            "reactions": {
                "by_emoji": {
                    "👍": { "emoji": "👍", "count": 1 }
                }
            }
        });
        assert_eq!(message_height(&msg, 80), 2, "1 content + 1 reaction");
    }

    #[test]
    fn message_height_no_reactions() {
        let msg =
            json!({"content": "hi", "author": "a", "created_at_local": "2026-01-01 10:00:00"});
        assert_eq!(message_height(&msg, 80), 1);
    }

    #[test]
    fn format_message_own_same_layout() {
        let msg = json!({
            "content": "hello",
            "author": "me",
            "created_at_local": "2026-01-01 10:00:00"
        });
        let lines = format_message(&msg, Some("me"));
        // Own messages use same layout: [HH:MM] author: content
        assert_eq!(lines.len(), 1);
        // First span is timestamp, second is author, third is content
        assert_eq!(lines[0].spans.len(), 3);
    }
}
