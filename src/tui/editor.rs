use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_width::UnicodeWidthChar as _;

#[derive(Clone, Default)]
pub(super) struct Editor {
    chars: Vec<char>,
    cursor: usize,
}

impl Editor {
    pub(super) fn from(value: &str) -> Self {
        let chars: Vec<_> = value.chars().collect();
        let cursor = chars.len();
        Self { chars, cursor }
    }

    pub(super) fn value(&self) -> String {
        self.chars.iter().collect()
    }

    pub(super) fn replace(&mut self, value: &str) {
        self.chars = value.chars().collect();
        self.cursor = self.chars.len();
    }

    pub(super) fn clear(&mut self) {
        self.chars.clear();
        self.cursor = 0;
    }

    pub(super) fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    pub(super) fn insert_str(&mut self, value: &str, multiline: bool) {
        let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
        for character in normalized
            .chars()
            .filter(|character| multiline || !matches!(character, '\n' | '\t'))
        {
            self.chars.insert(self.cursor, character);
            self.cursor += 1;
        }
    }

    pub(super) fn handle_key(&mut self, key: KeyEvent, multiline: bool, numeric: bool) -> bool {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return match key.code {
                KeyCode::Char('u') => {
                    self.clear();
                    true
                }
                KeyCode::Char('w') if !numeric => {
                    self.delete_previous_word();
                    true
                }
                _ => false,
            };
        }

        match key.code {
            KeyCode::Char(character)
                if !key.modifiers.contains(KeyModifiers::ALT)
                    && (!numeric || character.is_ascii_digit()) =>
            {
                self.chars.insert(self.cursor, character);
                self.cursor += 1;
                true
            }
            KeyCode::Backspace if self.cursor > 0 => {
                self.cursor -= 1;
                self.chars.remove(self.cursor);
                true
            }
            KeyCode::Delete if self.cursor < self.chars.len() => {
                self.chars.remove(self.cursor);
                true
            }
            KeyCode::Left => {
                self.cursor = self.cursor.saturating_sub(1);
                true
            }
            KeyCode::Right => {
                self.cursor = (self.cursor + 1).min(self.chars.len());
                true
            }
            KeyCode::Home => {
                self.cursor = self.current_line_start();
                true
            }
            KeyCode::End => {
                self.cursor = self.current_line_end();
                true
            }
            KeyCode::Up if multiline => {
                self.move_vertical(-1);
                true
            }
            KeyCode::Down if multiline => {
                self.move_vertical(1);
                true
            }
            KeyCode::Enter if multiline => {
                self.chars.insert(self.cursor, '\n');
                self.cursor += 1;
                true
            }
            _ => false,
        }
    }

    pub(super) fn cursor_line_column(&self) -> (usize, usize) {
        let start = self.current_line_start();
        let line = self.chars[..start]
            .iter()
            .filter(|character| **character == '\n')
            .count();
        let column = self.chars[start..self.cursor]
            .iter()
            .map(|character| character.width().unwrap_or(0))
            .sum();
        (line, column)
    }

    fn current_line_start(&self) -> usize {
        self.chars[..self.cursor]
            .iter()
            .rposition(|character| *character == '\n')
            .map_or(0, |index| index + 1)
    }

    fn current_line_end(&self) -> usize {
        self.chars[self.cursor..]
            .iter()
            .position(|character| *character == '\n')
            .map_or(self.chars.len(), |index| self.cursor + index)
    }

    fn move_vertical(&mut self, direction: i8) {
        let start = self.current_line_start();
        let column = self.cursor - start;
        if direction < 0 {
            if start == 0 {
                return;
            }
            let previous_end = start - 1;
            let previous_start = self.chars[..previous_end]
                .iter()
                .rposition(|character| *character == '\n')
                .map_or(0, |index| index + 1);
            self.cursor = previous_start + column.min(previous_end - previous_start);
        } else {
            let end = self.current_line_end();
            if end == self.chars.len() {
                return;
            }
            let next_start = end + 1;
            let next_end = self.chars[next_start..]
                .iter()
                .position(|character| *character == '\n')
                .map_or(self.chars.len(), |index| next_start + index);
            self.cursor = next_start + column.min(next_end - next_start);
        }
    }

    fn delete_previous_word(&mut self) {
        while self.cursor > 0 && self.chars[self.cursor - 1].is_whitespace() {
            self.cursor -= 1;
            self.chars.remove(self.cursor);
        }
        while self.cursor > 0 && !self.chars[self.cursor - 1].is_whitespace() {
            self.cursor -= 1;
            self.chars.remove(self.cursor);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_cursor_uses_terminal_width() {
        let editor = Editor::from("a界👋");
        assert_eq!(editor.cursor_line_column(), (0, 5));
    }

    #[test]
    fn numeric_editor_ignores_non_digits() {
        let mut editor = Editor::default();
        editor.handle_key(
            KeyEvent::new(KeyCode::Char('4'), KeyModifiers::NONE),
            false,
            true,
        );
        editor.handle_key(
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
            false,
            true,
        );
        assert_eq!(editor.value(), "4");
    }
}
