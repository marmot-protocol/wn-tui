use std::collections::HashMap;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, StatefulWidget, Widget},
};
use serde_json::Value;

use std::sync::LazyLock;

static EMPTY_MAP: LazyLock<HashMap<String, usize>> = LazyLock::new(HashMap::new);

/// Extract display name from a chat JSON value.
fn chat_name(chat: &Value) -> &str {
    chat.get("name")
        .or_else(|| chat.get("group_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown")
}

/// Extract last message preview from a chat JSON value.
fn last_message(chat: &Value) -> Option<&str> {
    chat.get("last_message")
        .or_else(|| chat.get("content"))
        .and_then(|v| v.as_str())
}

/// Extract group ID from a chat JSON value.
/// The CLI returns mls_group_id as either a hex string or an object like
/// `{"value": {"vec": [u8, ...]}}`. We normalize both to a hex string
/// since CLI commands expect hex-encoded group IDs.
pub fn group_id(chat: &Value) -> Option<String> {
    let val = chat.get("mls_group_id").or_else(|| chat.get("group_id"))?;
    if let Some(s) = val.as_str() {
        return Some(s.to_string());
    }
    // Handle object format: {"value": {"vec": [u8, ...]}}
    let bytes = val
        .get("value")
        .and_then(|v| v.get("vec"))
        .and_then(|v| v.as_array())?;
    let hex: String = bytes
        .iter()
        .filter_map(|b| b.as_u64().map(|n| format!("{:02x}", n)))
        .collect();
    if hex.is_empty() {
        None
    } else {
        Some(hex)
    }
}

/// Renders the chat list sidebar.
pub struct ChatListWidget<'a> {
    chats: &'a [Value],
    selected: usize,
    focused: bool,
    block: Option<Block<'a>>,
    unread: &'a HashMap<String, usize>,
}

impl<'a> ChatListWidget<'a> {
    pub fn new(chats: &'a [Value], selected: usize) -> Self {
        Self {
            chats,
            selected,
            focused: false,
            block: None,
            unread: &EMPTY_MAP,
        }
    }

    pub fn unread(mut self, unread: &'a HashMap<String, usize>) -> Self {
        self.unread = unread;
        self
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }
}

impl Widget for ChatListWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let border_color = if self.focused {
            Color::Cyan
        } else {
            Color::DarkGray
        };

        let block = self
            .block
            .unwrap_or_default()
            .border_style(Style::default().fg(border_color));

        let items: Vec<ListItem> = self
            .chats
            .iter()
            .enumerate()
            .map(|(i, chat)| {
                let name = chat_name(chat);
                let preview = last_message(chat).unwrap_or("");
                let unread_count = group_id(chat)
                    .as_ref()
                    .and_then(|gid| self.unread.get(gid.as_str()).copied())
                    .unwrap_or(0);

                let is_selected = i == self.selected;
                let marker = if is_selected { ">" } else { " " };

                let name_style = if is_selected {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else if unread_count > 0 {
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };

                let mut name_spans = vec![
                    Span::styled(format!("{marker} "), Style::default().fg(Color::Cyan)),
                    Span::styled(name.to_string(), name_style),
                ];

                if unread_count > 0 {
                    name_spans.push(Span::styled(
                        format!(" ({unread_count})"),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ));
                }

                let mut lines = vec![Line::from(name_spans)];

                if !preview.is_empty() {
                    let truncated: String = preview.chars().take(30).collect();
                    lines.push(Line::from(Span::styled(
                        format!("  {truncated}"),
                        Style::default().fg(Color::DarkGray),
                    )));
                }

                ListItem::new(lines)
            })
            .collect();

        let list = List::new(items).block(block);

        let mut state = ListState::default();
        state.select(Some(self.selected));

        StatefulWidget::render(list, area, buf, &mut state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_chat_name() {
        assert_eq!(chat_name(&json!({"name": "Coffee Chat"})), "Coffee Chat");
        assert_eq!(chat_name(&json!({"group_name": "Work"})), "Work");
        assert_eq!(chat_name(&json!({})), "Unknown");
    }

    #[test]
    fn extracts_group_id() {
        assert_eq!(
            group_id(&json!({"mls_group_id": "abc123"})),
            Some("abc123".to_string())
        );
        assert_eq!(
            group_id(&json!({"group_id": "def456"})),
            Some("def456".to_string())
        );
        assert_eq!(group_id(&json!({})), None);
        // Object format (as returned by the real CLI)
        assert_eq!(
            group_id(&json!({"mls_group_id": {"value": {"vec": [102, 169, 108]}}})),
            Some("66a96c".to_string())
        );
    }

    #[test]
    fn extracts_last_message() {
        assert_eq!(
            last_message(&json!({"last_message": "Hello!"})),
            Some("Hello!")
        );
        assert_eq!(last_message(&json!({})), None);
    }

    #[test]
    fn group_id_empty_byte_vec_returns_none() {
        assert_eq!(
            group_id(&json!({"mls_group_id": {"value": {"vec": []}}})),
            None
        );
    }

    #[test]
    fn group_id_non_object_non_string_returns_none() {
        assert_eq!(group_id(&json!({"mls_group_id": 42})), None);
        assert_eq!(group_id(&json!({"mls_group_id": true})), None);
        assert_eq!(group_id(&json!({"mls_group_id": null})), None);
    }

    #[test]
    fn group_id_prefers_mls_group_id_over_group_id() {
        assert_eq!(
            group_id(&json!({"mls_group_id": "preferred", "group_id": "fallback"})),
            Some("preferred".to_string())
        );
    }
}
