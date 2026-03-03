use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Widget},
};

/// Text input field with cursor tracking.
#[derive(Debug, Clone)]
pub struct Input {
    pub value: String,
    pub cursor: usize,
    pub masked: bool,
}

impl Input {
    pub fn new() -> Self {
        Self {
            value: String::new(),
            cursor: 0,
            masked: false,
        }
    }

    pub fn set_masked(&mut self, masked: bool) {
        self.masked = masked;
    }

    pub fn insert(&mut self, ch: char) {
        self.value.insert(self.cursor, ch);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.value.remove(self.cursor);
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.value.len() {
            self.value.remove(self.cursor);
        }
    }

    pub fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.value.len() {
            self.cursor += 1;
        }
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.value.len();
    }

    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
    }

    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    fn display_value(&self) -> String {
        if self.masked {
            "*".repeat(self.value.len())
        } else {
            self.value.clone()
        }
    }
}

/// Renders an Input field. Borrows the Input state for drawing.
pub struct InputWidget<'a> {
    input: &'a Input,
    focused: bool,
    block: Option<Block<'a>>,
}

impl<'a> InputWidget<'a> {
    pub fn new(input: &'a Input) -> Self {
        Self {
            input,
            focused: false,
            block: None,
        }
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

impl Widget for InputWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let inner = if let Some(block) = &self.block {
            let inner = block.inner(area);
            block.clone().render(area, buf);
            inner
        } else {
            area
        };

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        let display = self.input.display_value();
        let cursor_style = if self.focused {
            Style::default()
                .fg(Color::Black)
                .bg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        // Split text at cursor for styling
        let (before, cursor_char, after) = if self.focused && self.input.cursor <= display.len() {
            let before = &display[..self.input.cursor];
            if self.input.cursor < display.len() {
                let cursor_ch = &display[self.input.cursor..self.input.cursor + 1];
                let after = &display[self.input.cursor + 1..];
                (before, cursor_ch, after)
            } else {
                (before, " ", "") // Cursor at end — show block cursor on space
            }
        } else {
            (display.as_str(), "", "")
        };

        let line = Line::from(vec![
            Span::raw(before.to_string()),
            Span::styled(cursor_char.to_string(), cursor_style),
            Span::raw(after.to_string()),
        ]);

        Paragraph::new(line).render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_input_is_empty() {
        let input = Input::new();
        assert!(input.is_empty());
        assert_eq!(input.cursor, 0);
    }

    #[test]
    fn insert_characters() {
        let mut input = Input::new();
        input.insert('h');
        input.insert('i');
        assert_eq!(input.value, "hi");
        assert_eq!(input.cursor, 2);
    }

    #[test]
    fn backspace_removes_before_cursor() {
        let mut input = Input::new();
        input.insert('a');
        input.insert('b');
        input.insert('c');
        input.backspace();
        assert_eq!(input.value, "ab");
        assert_eq!(input.cursor, 2);
    }

    #[test]
    fn backspace_at_start_does_nothing() {
        let mut input = Input::new();
        input.backspace();
        assert!(input.is_empty());
        assert_eq!(input.cursor, 0);
    }

    #[test]
    fn delete_removes_at_cursor() {
        let mut input = Input::new();
        input.insert('a');
        input.insert('b');
        input.insert('c');
        input.move_left();
        input.move_left();
        input.delete();
        assert_eq!(input.value, "ac");
        assert_eq!(input.cursor, 1);
    }

    #[test]
    fn delete_at_end_does_nothing() {
        let mut input = Input::new();
        input.insert('a');
        input.delete();
        assert_eq!(input.value, "a");
    }

    #[test]
    fn cursor_movement() {
        let mut input = Input::new();
        input.insert('a');
        input.insert('b');
        input.insert('c');
        assert_eq!(input.cursor, 3);

        input.move_left();
        assert_eq!(input.cursor, 2);

        input.home();
        assert_eq!(input.cursor, 0);

        input.move_left(); // Can't go below 0
        assert_eq!(input.cursor, 0);

        input.end();
        assert_eq!(input.cursor, 3);

        input.move_right(); // Can't go beyond len
        assert_eq!(input.cursor, 3);
    }

    #[test]
    fn insert_at_middle() {
        let mut input = Input::new();
        input.insert('a');
        input.insert('c');
        input.move_left();
        input.insert('b');
        assert_eq!(input.value, "abc");
        assert_eq!(input.cursor, 2);
    }

    #[test]
    fn clear_resets_everything() {
        let mut input = Input::new();
        input.insert('x');
        input.insert('y');
        input.clear();
        assert!(input.is_empty());
        assert_eq!(input.cursor, 0);
    }

    #[test]
    fn masked_display() {
        let mut input = Input::new();
        input.set_masked(true);
        input.insert('s');
        input.insert('e');
        input.insert('c');
        assert_eq!(input.display_value(), "***");
        assert_eq!(input.value, "sec"); // Actual value preserved
    }
}
