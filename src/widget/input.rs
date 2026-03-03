use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Widget, Wrap},
};

/// Text input field with cursor tracking.
/// Cursor is a character index (not byte index) for correct Unicode handling.
#[derive(Debug, Clone)]
pub struct Input {
    pub value: String,
    /// Cursor position as character index (0 = before first char).
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

    /// Byte offset for the current cursor character position.
    fn cursor_byte_offset(&self) -> usize {
        self.value
            .char_indices()
            .nth(self.cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.value.len())
    }

    pub fn insert(&mut self, ch: char) {
        let byte_pos = self.cursor_byte_offset();
        self.value.insert(byte_pos, ch);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            let byte_pos = self.cursor_byte_offset();
            self.value.remove(byte_pos);
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.value.chars().count() {
            let byte_pos = self.cursor_byte_offset();
            self.value.remove(byte_pos);
        }
    }

    pub fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.value.chars().count() {
            self.cursor += 1;
        }
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.value.chars().count();
    }

    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
    }

    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    /// Returns the number of visual lines this input would occupy at the given width.
    pub fn line_count(&self, width: u16) -> u16 {
        if width == 0 {
            return 1;
        }
        let char_count = self.value.chars().count().max(1); // at least 1 for cursor
        char_count.div_ceil(width as usize) as u16
    }

    fn display_value(&self) -> String {
        if self.masked {
            "*".repeat(self.value.chars().count())
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

        // Split at cursor using character boundaries
        let char_count = display.chars().count();
        let cursor_pos = self.input.cursor.min(char_count);
        let byte_before: usize = display.chars().take(cursor_pos).map(|c| c.len_utf8()).sum();

        let before = &display[..byte_before];
        let (cursor_char, after) = if self.focused && cursor_pos < char_count {
            let ch = display[byte_before..].chars().next().unwrap();
            let byte_end = byte_before + ch.len_utf8();
            (&display[byte_before..byte_end], &display[byte_end..])
        } else if self.focused {
            (" ", "") // Cursor at end — show block cursor on space
        } else {
            ("", "")
        };

        let line = Line::from(vec![
            Span::raw(before.to_string()),
            Span::styled(cursor_char.to_string(), cursor_style),
            Span::raw(after.to_string()),
        ]);

        Paragraph::new(line)
            .wrap(Wrap { trim: false })
            .render(inner, buf);
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

    // ── Unicode / multi-byte tests ──────────────────────────────────

    #[test]
    fn insert_multibyte_chars() {
        let mut input = Input::new();
        input.insert('h');
        input.insert('\u{2019}'); // right single quote '
        input.insert('s');
        assert_eq!(input.value, "h\u{2019}s");
        assert_eq!(input.cursor, 3);
    }

    #[test]
    fn backspace_multibyte_char() {
        let mut input = Input::new();
        input.insert('a');
        input.insert('\u{2014}'); // em dash —
        input.insert('b');
        input.backspace(); // remove 'b'
        assert_eq!(input.value, "a\u{2014}");
        input.backspace(); // remove em dash
        assert_eq!(input.value, "a");
        assert_eq!(input.cursor, 1);
    }

    #[test]
    fn cursor_movement_with_multibyte() {
        let mut input = Input::new();
        input.insert('\u{1F600}'); // 😀 (4 bytes)
        input.insert('a');
        assert_eq!(input.cursor, 2);

        input.move_left();
        assert_eq!(input.cursor, 1);
        input.insert('b');
        assert_eq!(input.value, "\u{1F600}ba");
        assert_eq!(input.cursor, 2);
    }

    #[test]
    fn delete_multibyte_at_cursor() {
        let mut input = Input::new();
        input.insert('a');
        input.insert('\u{2019}'); // '
        input.insert('b');
        input.move_left();
        input.move_left();
        input.delete(); // delete the '
        assert_eq!(input.value, "ab");
    }

    // ── Render tests (crash prevention) ─────────────────────────────

    /// Helper: render an InputWidget into a buffer and return it without panicking.
    fn render_input(input: &Input, focused: bool) -> Buffer {
        let area = Rect::new(0, 0, 40, 1);
        let mut buf = Buffer::empty(area);
        InputWidget::new(input)
            .focused(focused)
            .render(area, &mut buf);
        buf
    }

    #[test]
    fn render_with_multibyte_cursor_at_end() {
        let mut input = Input::new();
        // The exact crash scenario: ASCII text followed by em dash
        for ch in "Stop hedging with \u{2018}it depends\u{2019} \u{2014}".chars() {
            input.insert(ch);
        }
        // Cursor is at the end, after the em dash
        render_input(&input, true); // must not panic
    }

    #[test]
    fn render_with_cursor_before_multibyte() {
        let mut input = Input::new();
        input.insert('a');
        input.insert('\u{2014}'); // —
        input.insert('b');
        input.move_left();
        input.move_left(); // cursor on the em dash
        render_input(&input, true); // must not panic
    }

    #[test]
    fn render_with_emoji() {
        let mut input = Input::new();
        input.insert('\u{1F600}'); // 😀 (4 bytes)
        input.insert(' ');
        input.insert('\u{1F525}'); // 🔥 (4 bytes)
        input.home();
        render_input(&input, true); // cursor on emoji, must not panic
    }

    #[test]
    fn render_long_mixed_content() {
        let mut input = Input::new();
        let text = "Don\u{2019}t use \u{2018}it depends\u{2019} \u{2014} commit to a take.";
        for ch in text.chars() {
            input.insert(ch);
        }
        assert_eq!(input.value, text);
        // Render with cursor at various positions
        render_input(&input, true);
        input.home();
        render_input(&input, true);
        input.end();
        input.move_left();
        render_input(&input, true);
    }

    #[test]
    fn render_unfocused_with_multibyte() {
        let mut input = Input::new();
        for ch in "quotes: \u{201C}hello\u{201D}".chars() {
            input.insert(ch);
        }
        render_input(&input, false); // unfocused path, must not panic
    }

    // ── line_count tests ──────────────────────────────────────────────

    #[test]
    fn line_count_empty_is_one() {
        let input = Input::new();
        assert_eq!(input.line_count(40), 1);
    }

    #[test]
    fn line_count_short_text_is_one() {
        let mut input = Input::new();
        for ch in "hello".chars() {
            input.insert(ch);
        }
        assert_eq!(input.line_count(40), 1);
    }

    #[test]
    fn line_count_exact_width_is_one() {
        let mut input = Input::new();
        for _ in 0..20 {
            input.insert('a');
        }
        assert_eq!(input.line_count(20), 1);
    }

    #[test]
    fn line_count_wraps_at_width() {
        let mut input = Input::new();
        for _ in 0..21 {
            input.insert('a');
        }
        assert_eq!(input.line_count(20), 2);
    }

    #[test]
    fn line_count_multiple_wraps() {
        let mut input = Input::new();
        for _ in 0..50 {
            input.insert('x');
        }
        assert_eq!(input.line_count(20), 3);
    }

    #[test]
    fn line_count_zero_width_is_one() {
        let mut input = Input::new();
        input.insert('a');
        assert_eq!(input.line_count(0), 1);
    }
}
