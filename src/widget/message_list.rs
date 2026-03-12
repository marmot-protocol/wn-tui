use std::collections::HashMap;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Widget, Wrap},
};
use serde_json::Value;
use unicode_width::UnicodeWidthStr;

use crate::app::MediaDownload;
use ratatui_image::protocol::StatefulProtocol;

/// Height in terminal rows for inline image thumbnails.
const INLINE_IMAGE_ROWS: u16 = 8;

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

use crate::util::extract_hash_hex;

/// Extract display filename from an attachment.
fn attachment_filename(att: &Value) -> String {
    // Try file_metadata.original_filename, then direct filename, then blossom_url basename
    att.get("file_metadata")
        .and_then(|m| m.get("original_filename").or_else(|| m.get("filename")))
        .and_then(|v| v.as_str())
        .or_else(|| att.get("filename").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
        .or_else(|| {
            att.get("blossom_url")
                .and_then(|v| v.as_str())
                .and_then(|url| url.rsplit('/').next())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "file".to_string())
}

/// Format attachment placeholder lines for a message.
/// For images with an available inline protocol, emits blank rows (the actual
/// image is rendered separately by the caller). Otherwise emits text placeholders.
fn format_attachments(
    msg: &Value,
    indent: usize,
    media_downloads: Option<&HashMap<String, MediaDownload>>,
    inline_images: Option<&HashMap<String, StatefulProtocol>>,
) -> Vec<Line<'static>> {
    let attachments = match msg.get("media_attachments").and_then(|v| v.as_array()) {
        Some(arr) if !arr.is_empty() => arr,
        _ => return vec![],
    };
    let mut lines = Vec::new();
    for att in attachments {
        let mime = att.get("mime_type").and_then(|v| v.as_str()).unwrap_or("");
        let filename = attachment_filename(att);
        let is_image = mime.starts_with("image/");

        if !is_image {
            lines.push(Line::from(vec![
                Span::raw(" ".repeat(indent)),
                Span::styled(
                    format!("[file {filename}]"),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
            continue;
        }

        // Try original_file_hash, file_hash, encrypted_file_hash
        let hash_hex = extract_hash_hex(att, "original_file_hash")
            .or_else(|| extract_hash_hex(att, "file_hash"))
            .or_else(|| extract_hash_hex(att, "encrypted_file_hash"))
            .unwrap_or_default();

        // If we have an inline image, reserve blank rows (rendered later)
        let has_inline = inline_images
            .map(|m| m.contains_key(&hash_hex))
            .unwrap_or(false);
        if has_inline {
            for _ in 0..INLINE_IMAGE_ROWS {
                lines.push(Line::raw(""));
            }
            continue;
        }

        let status = media_downloads.and_then(|m| m.get(&hash_hex));
        let label = match status {
            Some(MediaDownload::Downloading) => format!("[downloading {filename}...]"),
            Some(MediaDownload::Downloaded(_)) => format!("[loading {filename}...]"),
            Some(MediaDownload::Failed(err)) => format!("[{filename} failed: {err}]"),
            None => format!("[img {filename}]"),
        };
        let color = match status {
            Some(MediaDownload::Downloading) => Color::Yellow,
            Some(MediaDownload::Downloaded(_)) => Color::Cyan,
            Some(MediaDownload::Failed(_)) => Color::Red,
            None => Color::DarkGray,
        };

        lines.push(Line::from(vec![
            Span::raw(" ".repeat(indent)),
            Span::styled(label, Style::default().fg(color)),
        ]));
    }
    lines
}

/// Check if a message is a reply and return the replied-to event ID.
fn reply_to_id(msg: &Value) -> Option<&str> {
    let reply_id = msg.get("reply_to").and_then(|v| v.as_str())?;
    if reply_id.is_empty() {
        return None;
    }
    Some(reply_id)
}

/// Format a reply indicator line by looking up the parent message.
fn format_reply_line(reply_id: &str, messages: &[Value], indent: usize) -> Line<'static> {
    let parent = messages
        .iter()
        .find(|m| m.get("id").and_then(|v| v.as_str()) == Some(reply_id));

    let label = if let Some(parent) = parent {
        let name = author_name(parent);
        let preview: String = content(parent).chars().take(30).collect();
        if preview.chars().count() >= 30 {
            format!("reply to {name}: \"{preview}...\"")
        } else {
            format!("reply to {name}: \"{preview}\"")
        }
    } else {
        let short_id = if reply_id.len() > 12 {
            format!("{}...", &reply_id[..12])
        } else {
            reply_id.to_string()
        };
        format!("reply to {short_id}")
    };

    Line::from(vec![
        Span::raw(" ".repeat(indent)),
        Span::styled(
            format!("\u{21a9} {label}"),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ),
    ])
}

/// Build the display lines for a message.
/// All messages use the same layout: `[HH:MM] author: content`
/// Own messages are distinguished by green author color.
fn format_message(
    msg: &Value,
    my_pubkey: Option<&str>,
    media_downloads: Option<&HashMap<String, MediaDownload>>,
    inline_images: Option<&HashMap<String, StatefulProtocol>>,
    all_messages: &[Value],
) -> Vec<Line<'static>> {
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

    // Reply indicator (before content)
    if let Some(reply_id) = reply_to_id(msg) {
        lines.push(format_reply_line(reply_id, all_messages, indent));
    }

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

    lines.extend(format_attachments(
        msg,
        indent,
        media_downloads,
        inline_images,
    ));

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
/// Calculate total rows needed for attachments, accounting for inline images.
fn attachment_rows(
    msg: &Value,
    inline_images: Option<&HashMap<String, StatefulProtocol>>,
) -> usize {
    let attachments = match msg.get("media_attachments").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return 0,
    };
    let mut rows = 0;
    for att in attachments {
        let mime = att.get("mime_type").and_then(|v| v.as_str()).unwrap_or("");
        let is_image = mime.starts_with("image/");
        if is_image {
            let hash = extract_hash_hex(att, "original_file_hash")
                .or_else(|| extract_hash_hex(att, "file_hash"))
                .or_else(|| extract_hash_hex(att, "encrypted_file_hash"))
                .unwrap_or_default();
            let has_inline = inline_images
                .map(|m| m.contains_key(&hash))
                .unwrap_or(false);
            rows += if has_inline {
                INLINE_IMAGE_ROWS as usize
            } else {
                1
            };
        } else {
            rows += 1; // text placeholder for non-image
        }
    }
    rows
}

fn message_height_with_images(
    msg: &Value,
    width: usize,
    inline_images: Option<&HashMap<String, StatefulProtocol>>,
) -> usize {
    let ts = timestamp(msg);
    let author = author_name(msg);
    let text = content(msg);

    let prefix_width = format!("[{ts}] ").width() + format!("{author}: ").width();
    let content_lines: Vec<&str> = text.split('\n').collect();

    let mut total_rows = 0;

    if reply_to_id(msg).is_some() {
        total_rows += 1;
    }

    for line_text in &content_lines {
        total_rows += wrapped_line_count(prefix_width + line_text.width(), width);
    }

    if has_reactions(msg) {
        total_rows += 1;
    }

    total_rows += attachment_rows(msg, inline_images);

    total_rows.max(1)
}

#[cfg(test)]
fn message_height(msg: &Value, width: usize) -> usize {
    message_height_with_images(msg, width, None)
}

/// Renders the message list.
/// `scroll_from_bottom` is how many messages to scroll up from the bottom (0 = at bottom).
pub struct MessageListWidget<'a> {
    messages: &'a [Value],
    scroll_from_bottom: usize,
    block: Option<Block<'a>>,
    my_pubkey: Option<&'a str>,
    /// If set, highlight this message index (absolute index into messages slice).
    selected: Option<usize>,
    loading: bool,
    media_downloads: Option<&'a HashMap<String, MediaDownload>>,
    inline_images: Option<&'a HashMap<String, StatefulProtocol>>,
}

impl<'a> MessageListWidget<'a> {
    pub fn new(messages: &'a [Value], scroll_from_bottom: usize) -> Self {
        Self {
            messages,
            scroll_from_bottom,
            block: None,
            my_pubkey: None,
            selected: None,
            loading: false,
            media_downloads: None,
            inline_images: None,
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

    pub fn selected(mut self, selected: Option<usize>) -> Self {
        self.selected = selected;
        self
    }

    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    pub fn media_downloads(mut self, downloads: &'a HashMap<String, MediaDownload>) -> Self {
        self.media_downloads = Some(downloads);
        self
    }

    pub fn inline_images(mut self, images: &'a HashMap<String, StatefulProtocol>) -> Self {
        self.inline_images = Some(images);
        self
    }
}

impl MessageListWidget<'_> {
    /// Compute which messages are visible and their Y positions.
    /// Returns (visible_msg_indices_with_y, inner_area).
    ///
    /// `scroll_from_bottom` determines the anchor: the newest message that should
    /// be visible. We then fill backward from the anchor to find how many older
    /// messages fit, and render forward from that start index to fill the viewport.
    fn compute_visible(&self, area: Rect) -> (Vec<(usize, usize)>, Rect) {
        let inner = if let Some(block) = &self.block {
            block.inner(area)
        } else {
            area
        };

        if inner.height == 0 || inner.width == 0 || self.messages.is_empty() {
            return (vec![], inner);
        }

        let visible_height = inner.height as usize;
        let width = inner.width as usize;
        let total = self.messages.len();
        let skip = self.scroll_from_bottom.min(total.saturating_sub(1));

        // The anchor is the newest message that should be on screen.
        let anchor = total.saturating_sub(1 + skip);

        // Walk backward from the anchor to find the topmost message that fits.
        let mut start = anchor;
        let mut used_rows =
            message_height_with_images(&self.messages[anchor], width, self.inline_images);
        for i in (0..anchor).rev() {
            let h = message_height_with_images(&self.messages[i], width, self.inline_images);
            if used_rows + h > visible_height {
                break;
            }
            used_rows += h;
            start = i;
        }

        // Render forward from start, filling the viewport top-down.
        let mut result = Vec::new();
        let mut y = inner.y as usize;
        let y_limit = inner.y as usize + visible_height;
        for i in start..total {
            let h = message_height_with_images(&self.messages[i], width, self.inline_images);
            if !result.is_empty() && y + h > y_limit {
                break;
            }
            result.push((i, y));
            y += h;
        }

        (result, inner)
    }

    /// Return the (first, last) visible message indices for this area.
    /// Returns (0, 0) if nothing is visible.
    pub fn visible_range(&self, area: Rect) -> (usize, usize) {
        let (visible, _) = self.compute_visible(area);
        match (visible.first(), visible.last()) {
            (Some(&(first, _)), Some(&(last, _))) => (first, last),
            _ => (0, 0),
        }
    }

    /// Compute the positions where inline images should be rendered.
    /// Returns (file_hash_hex, Rect) pairs.
    pub fn image_positions(&self, area: Rect) -> Vec<(String, Rect)> {
        let (visible, inner) = self.compute_visible(area);
        let width = inner.width as usize;
        let mut positions = Vec::new();

        for (idx, base_y) in visible {
            let msg = &self.messages[idx];
            let attachments = match msg.get("media_attachments").and_then(|v| v.as_array()) {
                Some(arr) => arr,
                None => continue,
            };

            // Compute the Y offset where attachments start (after reply + content + reactions)
            let text = content(msg);
            let ts = timestamp(msg);
            let author = author_name(msg);
            let ts_prefix = format!("[{ts}] ");
            let ts_cols = ts_prefix.width() as u16;
            let prefix_width = ts_prefix.width() + format!("{author}: ").width();
            let content_lines: Vec<&str> = text.split('\n').collect();
            let mut row_offset = 0usize;
            if reply_to_id(msg).is_some() {
                row_offset += 1;
            }
            for line_text in &content_lines {
                row_offset += wrapped_line_count(prefix_width + line_text.width(), width);
            }
            if has_reactions(msg) {
                row_offset += 1;
            }

            for att in attachments {
                let mime = att.get("mime_type").and_then(|v| v.as_str()).unwrap_or("");
                if !mime.starts_with("image/") {
                    row_offset += 1; // non-image placeholder line
                    continue;
                }
                let hash = extract_hash_hex(att, "original_file_hash")
                    .or_else(|| extract_hash_hex(att, "file_hash"))
                    .or_else(|| extract_hash_hex(att, "encrypted_file_hash"))
                    .unwrap_or_default();

                let has_inline = self
                    .inline_images
                    .map(|m| m.contains_key(&hash))
                    .unwrap_or(false);

                if has_inline {
                    let img_y = base_y + row_offset;
                    let img_x = inner.x + ts_cols;
                    let img_w = inner.width.saturating_sub(ts_cols);
                    positions.push((
                        hash,
                        Rect::new(img_x, img_y as u16, img_w, INLINE_IMAGE_ROWS),
                    ));
                    row_offset += INLINE_IMAGE_ROWS as usize;
                } else {
                    row_offset += 1; // text placeholder
                }
            }
        }

        positions
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
                let (text, color) = if self.loading {
                    ("Loading messages...", Color::Yellow)
                } else {
                    ("No messages yet", Color::DarkGray)
                };
                let empty =
                    Paragraph::new(Span::styled(text, Style::default().fg(color))).centered();
                empty.render(inner, buf);
            }
            return;
        }

        let visible_height = inner.height as usize;
        let width = inner.width as usize;
        let total = self.messages.len();
        let skip_messages = self.scroll_from_bottom.min(total);

        let mut visible_msgs: Vec<usize> = Vec::new();
        let mut used_rows = 0;

        let end = total.saturating_sub(skip_messages);
        for i in (0..end).rev() {
            let h = message_height_with_images(&self.messages[i], width, self.inline_images);
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
            let lines = format_message(
                msg,
                self.my_pubkey,
                self.media_downloads,
                self.inline_images,
                self.messages,
            );
            let h = message_height_with_images(msg, width, self.inline_images);
            let is_selected = self.selected == Some(idx);

            let msg_area = Rect::new(inner.x, y, inner.width, h as u16);

            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .render(msg_area, buf);

            if is_selected {
                for row in msg_area.y..msg_area.y + msg_area.height {
                    for col in msg_area.x..msg_area.x + msg_area.width {
                        if let Some(cell) = buf.cell_mut((col, row)) {
                            cell.set_bg(Color::DarkGray);
                            // Ensure fg contrast: DarkGray-on-DarkGray is invisible
                            if cell.fg == Color::DarkGray {
                                cell.set_fg(Color::Gray);
                            }
                        }
                    }
                }
            }

            y += h as u16;
            if y >= inner.y + inner.height {
                break;
            }
        }
    }
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
        let lines = format_message(&msg, None, None, None, &[]);
        assert_eq!(lines.len(), 2, "Should produce 2 Line entries");
    }

    #[test]
    fn format_reactions_empty() {
        let msg =
            json!({"content": "hi", "author": "a", "created_at_local": "2026-01-01 10:00:00"});
        let lines = format_message(&msg, None, None, None, &[]);
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
        let lines = format_message(&msg, None, None, None, &[]);
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
        let lines = format_message(&msg, None, None, None, &[]);
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
        let lines = format_message(&msg, Some("me"), None, None, &[]);
        // Own messages use same layout: [HH:MM] author: content
        assert_eq!(lines.len(), 1);
        // First span is timestamp, second is author, third is content
        assert_eq!(lines[0].spans.len(), 3);
    }

    #[test]
    fn format_message_includes_attachment_lines() {
        let downloads = HashMap::from([(
            "hash1".to_string(),
            MediaDownload::Downloaded("/tmp/photo.png".into()),
        )]);
        let msg = json!({
            "content": "check this",
            "author": "alice",
            "created_at_local": "2026-01-01 10:00:00",
            "media_attachments": [
                {
                    "original_file_hash": "hash1",
                    "mime_type": "image/png",
                    "filename": "photo.png"
                }
            ]
        });
        let lines = format_message(&msg, None, Some(&downloads), None, &[]);
        // 1 content line + 1 attachment line
        assert_eq!(lines.len(), 2);
        let att_text: String = lines[1].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            att_text.contains("[loading photo.png...]"),
            "got: {att_text}"
        );
    }

    #[test]
    fn format_message_shows_downloading_status() {
        let downloads = HashMap::from([("hash1".to_string(), MediaDownload::Downloading)]);
        let msg = json!({
            "content": "wait",
            "author": "a",
            "created_at_local": "2026-01-01 10:00:00",
            "media_attachments": [
                {
                    "original_file_hash": "hash1",
                    "mime_type": "image/jpeg",
                    "filename": "img.jpg"
                }
            ]
        });
        let lines = format_message(&msg, None, Some(&downloads), None, &[]);
        let att_text: String = lines
            .last()
            .unwrap()
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(att_text.contains("downloading"), "got: {att_text}");
    }

    #[test]
    fn format_message_shows_non_image_as_file() {
        let msg = json!({
            "content": "doc",
            "author": "a",
            "created_at_local": "2026-01-01 10:00:00",
            "media_attachments": [
                {
                    "original_file_hash": "hash1",
                    "mime_type": "application/pdf",
                    "filename": "doc.pdf"
                }
            ]
        });
        let lines = format_message(&msg, None, None, None, &[]);
        let att_text: String = lines
            .last()
            .unwrap()
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(att_text.contains("[file doc.pdf]"), "got: {att_text}");
    }

    #[test]
    fn message_height_includes_attachments() {
        let msg = json!({
            "content": "img",
            "author": "a",
            "created_at_local": "2026-01-01 10:00:00",
            "media_attachments": [
                {
                    "file_hash": "h1",
                    "mime_type": "image/png",
                    "filename": "a.png"
                },
                {
                    "file_hash": "h2",
                    "mime_type": "image/gif",
                    "filename": "b.gif"
                }
            ]
        });
        // 1 content line + 2 attachment lines
        assert_eq!(message_height(&msg, 80), 3);
    }

    #[test]
    fn format_message_reply_shows_indicator() {
        let parent = json!({
            "id": "parent1",
            "content": "original message",
            "author": "alice",
            "display_name": "Alice",
            "created_at_local": "2026-01-01 09:00:00"
        });
        let reply = json!({
            "id": "reply1",
            "content": "my reply",
            "author": "bob",
            "reply_to": "parent1",
            "is_reply": true,
            "created_at_local": "2026-01-01 10:00:00"
        });
        let messages = vec![parent, reply.clone()];
        let lines = format_message(&reply, None, None, None, &messages);
        // Should have reply indicator line + content line = 2 lines
        assert_eq!(lines.len(), 2);
        let reply_text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            reply_text.contains("Alice"),
            "should show parent author: {reply_text}"
        );
        assert!(
            reply_text.contains("original message"),
            "should show parent content: {reply_text}"
        );
    }

    #[test]
    fn format_message_reply_fallback_when_parent_missing() {
        let reply = json!({
            "id": "reply1",
            "content": "orphaned reply",
            "author": "bob",
            "reply_to": "unknown_event_id_12345",
            "is_reply": true,
            "created_at_local": "2026-01-01 10:00:00"
        });
        let lines = format_message(&reply, None, None, None, &[]);
        assert_eq!(lines.len(), 2);
        let reply_text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            reply_text.contains("unknown_even"),
            "should show truncated event id: {reply_text}"
        );
    }

    #[test]
    fn message_height_includes_reply_line() {
        let msg = json!({
            "content": "reply text",
            "author": "a",
            "reply_to": "parent1",
            "is_reply": true,
            "created_at_local": "2026-01-01 10:00:00"
        });
        // 1 reply indicator + 1 content = 2
        assert_eq!(message_height(&msg, 80), 2);
    }

    #[test]
    fn non_reply_no_indicator() {
        let msg = json!({
            "content": "normal",
            "author": "a",
            "reply_to": null,
            "is_reply": false,
            "created_at_local": "2026-01-01 10:00:00"
        });
        let lines = format_message(&msg, None, None, None, &[]);
        assert_eq!(lines.len(), 1);
        assert_eq!(message_height(&msg, 80), 1);
    }

    // ── compute_visible / visible_range tests ──────────────────────────

    /// Build N short (1-row) messages for viewport tests.
    fn short_msgs(n: usize) -> Vec<Value> {
        (0..n)
            .map(|i| {
                json!({
                    "content": format!("msg{i}"),
                    "author": "a",
                    "created_at_local": "2026-01-01 10:00:00"
                })
            })
            .collect()
    }

    /// Helper: compute visible_range for given messages, scroll, and viewport.
    fn vis_range(msgs: &[Value], scroll: usize, height: u16, width: u16) -> (usize, usize) {
        let w = MessageListWidget::new(msgs, scroll);
        w.visible_range(Rect::new(0, 0, width, height))
    }

    #[test]
    fn visible_range_at_bottom_shows_newest() {
        let msgs = short_msgs(20);
        let (first, last) = vis_range(&msgs, 0, 10, 80);
        assert_eq!(last, 19, "last visible should be the newest message");
        assert_eq!(first, 10, "first visible should be 10 messages from end");
    }

    #[test]
    fn visible_range_scroll_to_top_fills_forward() {
        // Regression: pressing 'g' sets scroll = total - 1. This must show
        // message 0 at the top and fill the viewport forward — not just 1 message.
        let msgs = short_msgs(20);
        let scroll = msgs.len() - 1; // = 19, same as pressing 'g'
        let (first, last) = vis_range(&msgs, scroll, 10, 80);
        assert_eq!(first, 0, "should start at the oldest message");
        assert!(
            last >= 9,
            "should fill viewport with ~10 messages, got last={last}"
        );
    }

    #[test]
    fn visible_range_mid_scroll() {
        let msgs = short_msgs(30);
        let (first, last) = vis_range(&msgs, 10, 10, 80);
        // anchor = 29 - 10 = 19, backward fill 10 msgs, forward from 10
        assert_eq!(last, 19);
        assert_eq!(first, 10);
    }

    #[test]
    fn visible_range_all_messages_fit() {
        let msgs = short_msgs(5);
        let (first, last) = vis_range(&msgs, 0, 20, 80);
        assert_eq!(first, 0);
        assert_eq!(last, 4, "all 5 messages should be visible");
    }

    #[test]
    fn visible_range_oversized_message_not_blank() {
        // A message taller than the viewport must still produce a non-empty result.
        let msgs = vec![json!({
            "content": "x\n".repeat(50), // 50+ rows
            "author": "a",
            "created_at_local": "2026-01-01 10:00:00"
        })];
        let (first, last) = vis_range(&msgs, 0, 10, 80);
        assert_eq!(first, 0, "oversized message must still be visible");
        assert_eq!(last, 0);
    }

    #[test]
    fn visible_range_oversized_message_mid_list() {
        // Short messages around an oversized one: viewport should include it.
        let mut msgs = short_msgs(3);
        msgs.insert(
            1,
            json!({
                "content": "x\n".repeat(50),
                "author": "a",
                "created_at_local": "2026-01-01 10:00:00"
            }),
        );
        // total = 4 messages, anchor = msg 3 (scroll=0)
        // backward fill from msg 3: msg 2 fits (1 row), msg 1 (50+ rows) doesn't fit
        // start = 2, forward render: msg 2, msg 3
        let (_first, last) = vis_range(&msgs, 0, 10, 80);
        assert!(last == 3, "newest message should be visible");
        // When scrolling to the oversized message (anchor = 1):
        let (first2, last2) = vis_range(&msgs, 2, 10, 80);
        assert_eq!(first2, 1, "oversized msg should be first visible");
        assert!(last2 >= 1, "at least the oversized message is shown");
    }
}
