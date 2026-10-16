//! A one-line text field with a cursor, for typing topics.

use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LineEditor {
    text: String,
    /// Byte offset into `text`, always on a character boundary.
    cursor: usize,
}

impl LineEditor {
    /// A field holding `text`, with the cursor at the end.
    pub fn new(text: &str) -> Self {
        LineEditor {
            text: text.to_string(),
            cursor: text.len(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Display columns before the cursor, for placing the terminal cursor.
    pub fn cursor_column(&self) -> usize {
        self.text[..self.cursor].width()
    }

    pub fn insert(&mut self, c: char) {
        if c.is_control() {
            return;
        }
        self.text.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    pub fn left(&mut self) {
        if let Some(c) = self.text[..self.cursor].chars().next_back() {
            self.cursor -= c.len_utf8();
        }
    }

    pub fn right(&mut self) {
        if let Some(c) = self.text[self.cursor..].chars().next() {
            self.cursor += c.len_utf8();
        }
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.text.len();
    }

    pub fn backspace(&mut self) {
        if let Some(c) = self.text[..self.cursor].chars().next_back() {
            self.cursor -= c.len_utf8();
            self.text.remove(self.cursor);
        }
    }

    pub fn delete_forward(&mut self) {
        if self.cursor < self.text.len() {
            self.text.remove(self.cursor);
        }
    }

    /// Delete the word before the cursor and any spaces after it, like
    /// readline's Ctrl-W.
    pub fn delete_word(&mut self) {
        let before = self.text[..self.cursor].trim_end();
        let start = before
            .char_indices()
            .rev()
            .find(|(_, c)| c.is_whitespace())
            .map_or(0, |(i, c)| i + c.len_utf8());
        self.text.replace_range(start..self.cursor, "");
        self.cursor = start;
    }

    pub fn delete_to_start(&mut self) {
        self.text.replace_range(..self.cursor, "");
        self.cursor = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(text: &str) -> LineEditor {
        let mut editor = LineEditor::default();
        text.chars().for_each(|c| editor.insert(c));
        editor
    }

    #[test]
    fn typing_inserts_at_the_cursor() {
        let mut editor = typed("ac");
        editor.left();
        editor.insert('b');
        assert_eq!(editor.text(), "abc");
        assert_eq!(editor.cursor_column(), 2);
    }

    #[test]
    fn the_cursor_steps_over_whole_characters() {
        let mut editor = typed("é漢");
        editor.left();
        assert_eq!(editor.cursor_column(), 1);
        editor.backspace();
        assert_eq!(editor.text(), "漢");
        editor.right();
        editor.right();
        assert_eq!(editor.cursor_column(), 2);
    }

    #[test]
    fn delete_word_takes_the_word_and_trailing_spaces() {
        let mut editor = typed("one two  ");
        editor.delete_word();
        assert_eq!(editor.text(), "one ");
        editor.delete_word();
        assert_eq!(editor.text(), "");
    }

    #[test]
    fn delete_to_start_keeps_what_follows_the_cursor() {
        let mut editor = typed("abc");
        editor.left();
        editor.delete_to_start();
        assert_eq!(editor.text(), "c");
        assert_eq!(editor.cursor_column(), 0);
    }

    #[test]
    fn control_characters_are_not_typed() {
        assert_eq!(typed("a\tb\n").text(), "ab");
    }
}
