use std::mem;

use ratatui::{text::Line, style::Stylize};

pub struct InputLine {
    buffer: String,
    cursor_position: usize,
}

impl InputLine {
    pub fn new() -> InputLine {
        InputLine {
            buffer: String::new(),
            cursor_position: 0,
        }
    }

    pub fn get_buffer_and_clear(&mut self) -> String {
        self.cursor_position = 0;
        std::mem::take(&mut self.buffer)
    }

    pub fn as_line(&self) -> Line {
        Line::from(self.buffer.to_owned().white())
    }

    pub fn cursor_position(&self) -> usize {
        self.cursor_position
    }

    pub fn type_string(&mut self, stuff: String) {
        self.buffer = insert_string(
            mem::take(&mut self.buffer),
            &stuff,
            self.cursor_position);

        self.cursor_position += stuff.chars().count();
    }

    pub fn left(&mut self) {
        self.cursor_position = self.cursor_position.saturating_sub(1);
    }

    pub fn right(&mut self) {
        if self.cursor_position < self.buffer.chars().count() {
            self.cursor_position += 1;
        }
    }

    pub fn home(&mut self) {
        self.cursor_position = 0;
    }

    pub fn end(&mut self) {
        self.cursor_position = self.buffer.chars().count();
    }

    pub fn backspace(&mut self) {
        if self.cursor_position > 0 {
            self.buffer = del_from_string(
                std::mem::take(&mut self.buffer),
                self.cursor_position - 1);
            self.cursor_position = self.cursor_position.saturating_sub(1);
        }
    }

    pub fn delete(&mut self) {
        self.buffer = del_from_string(
            std::mem::take(&mut self.buffer),
            self.cursor_position);
    }
}

/// Insert a string into another at a given character (not byte) position.
///
/// Returns modified string.
/// If `position` is out of bounds, returns unmodified string.
///
/// Should not panic as out-of-bounds are checked for and respects codepoint boundaries.
fn insert_string(source: String, what: &String, position: usize) -> String {
    if position > source.chars().count() {
        return source.to_owned();
    }

    let byte_position = source.char_indices().nth(position).map_or(source.len(), |(idx, _)| idx);

    let (before, after) = source.split_at(byte_position);

    format!("{before}{what}{after}")
}

/// Remove a character from a string at a given character (not byte) position.
///
/// Returns modified string.
/// If `position` is out of bounds or `source` is empty, returns unmodified string.
///
/// Should not panic as out-of-bounds are checked for and respects codepoint boundaries.
fn del_from_string(source: String, position: usize) -> String {
    let source_len = source.chars().count();

    if source_len == 0 || position >= source_len {
        return source.to_owned();
    }

    let byte_position = source.char_indices().nth(position).map_or(source.len(), |(idx, _)| idx);

    let (before, after) = source.split_at(byte_position);

    format!("{before}{}", after.chars().skip(1).collect::<String>())
}