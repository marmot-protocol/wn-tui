use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap},
};

use crate::widget::input::{Input, InputWidget};

/// A centered popup overlay.
pub struct PopupWidget<'a> {
    title: &'a str,
    body: Vec<Line<'a>>,
    input: Option<&'a Input>,
    hints: Vec<(&'a str, &'a str)>,
    width: u16,
    height: u16,
}

impl<'a> PopupWidget<'a> {
    pub fn new(title: &'a str) -> Self {
        Self {
            title,
            body: Vec::new(),
            input: None,
            hints: Vec::new(),
            width: 50,
            height: 10,
        }
    }

    pub fn body(mut self, body: Vec<Line<'a>>) -> Self {
        self.body = body;
        self
    }

    pub fn input(mut self, input: &'a Input) -> Self {
        self.input = Some(input);
        self
    }

    pub fn hints(mut self, hints: Vec<(&'a str, &'a str)>) -> Self {
        self.hints = hints;
        self
    }

    pub fn size(mut self, width: u16, height: u16) -> Self {
        self.width = width;
        self.height = height;
        self
    }
}

impl Widget for PopupWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let popup_area = centered_rect(self.width, self.height, area);

        // Clear the area behind the popup
        Clear.render(popup_area, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(format!(" {} ", self.title))
            .title_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            );

        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        // Layout: body + optional input + hints
        let mut constraints = vec![];
        if !self.body.is_empty() {
            constraints.push(Constraint::Length(self.body.len() as u16));
        }
        if self.input.is_some() {
            if !self.body.is_empty() {
                constraints.push(Constraint::Length(1)); // spacer
            }
            constraints.push(Constraint::Length(3)); // input with border
        }
        constraints.push(Constraint::Fill(1)); // spacer
        if !self.hints.is_empty() {
            constraints.push(Constraint::Length(1)); // hints
        }

        let rows = Layout::vertical(constraints).split(inner);
        let mut row_idx = 0;

        // Body text
        let has_body = !self.body.is_empty();
        if has_body {
            Paragraph::new(self.body)
                .wrap(Wrap { trim: false })
                .render(rows[row_idx], buf);
            row_idx += 1;
        }

        // Input field
        if let Some(input) = self.input {
            if has_body {
                row_idx += 1; // skip spacer
            }
            let input_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::White));
            InputWidget::new(input)
                .focused(true)
                .block(input_block)
                .render(rows[row_idx], buf);
            row_idx += 1;
        }

        // Hints
        if !self.hints.is_empty() {
            let last_row = rows.len() - 1;
            let _ = row_idx; // skip to last
            let spans: Vec<Span> = self
                .hints
                .iter()
                .flat_map(|(key, desc)| {
                    vec![
                        Span::styled(format!("[{key}] "), Style::default().fg(Color::Cyan)),
                        Span::styled(format!("{desc}  "), Style::default().fg(Color::DarkGray)),
                    ]
                })
                .collect();
            Paragraph::new(Line::from(spans))
                .centered()
                .render(rows[last_row], buf);
        }
    }
}

/// Calculate a centered rectangle within the given area.
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let w = width.min(area.width.saturating_sub(2));
    let h = height.min(area.height.saturating_sub(2));
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centered_rect_fits_in_area() {
        let area = Rect::new(0, 0, 80, 24);
        let r = centered_rect(40, 10, area);
        assert_eq!(r.width, 40);
        assert_eq!(r.height, 10);
        assert_eq!(r.x, 20);
        assert_eq!(r.y, 7);
    }

    #[test]
    fn centered_rect_clamps_to_area() {
        let area = Rect::new(0, 0, 30, 10);
        let r = centered_rect(50, 20, area);
        assert!(r.width <= area.width);
        assert!(r.height <= area.height);
    }
}
