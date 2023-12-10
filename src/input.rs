use ratatui::{text::Line, style::Stylize};

use crate::ring::RingBuffer;

pub struct InputLine {
    state: InputState,

    history: RingBuffer<String>,
}

#[derive(Clone)]
enum InputState {
    Typing { buffer: String, cursor_position: usize },
    HistorySearch { search_term: String, index: usize },
}

impl InputState {
    fn empty_typing() -> InputState {
        InputState::Typing { buffer: String::new(), cursor_position: 0 }
    }

    fn typing_from_buffer(buffer: String) -> InputState {
        let cursor_position = buffer.chars().count();
        InputState::Typing { buffer, cursor_position }
    }
}

impl InputLine {
    pub fn new() -> InputLine {
        InputLine {
            state: InputState::empty_typing(),

            history: RingBuffer::new(1000),
        }
    }

    pub fn get_and_submit(&mut self) -> String {
        let (result, new_state) = match &mut self.state {
            InputState::Typing { buffer, cursor_position: _ } => {
                if !buffer.is_empty() {
                    self.history.find_and_push_back(buffer.clone());
                }

                (buffer.clone(), InputState::empty_typing())
            },
            InputState::HistorySearch { search_term, index } => {
                let submit = if search_term.is_empty() {
                    self.history.get(*index).clone().unwrap_or_default()
                } else {
                    search_term.to_string()
                };

                self.history.find_and_push_back(submit.clone());
                (submit, InputState::empty_typing())
            },
        };

        self.state = new_state;
        result.clone()
    }

    pub fn get_and_clear(&mut self) -> String {
        let (result, new_state) = match &mut self.state {
            InputState::Typing { buffer, cursor_position: _ } => {
                (buffer.clone(), InputState::empty_typing())
            },
            InputState::HistorySearch { search_term: term, index: _ } => {
                (term.clone(), InputState::empty_typing())
            },
        };

        self.state = new_state;
        result.clone()
    }

    pub fn as_line(&self) -> Line {
        match &self.state {
            InputState::Typing { buffer, cursor_position: _ } => {
                buffer.clone().white().into()
            },
            InputState::HistorySearch { search_term, index } => {
                let history_entry = self.history.get(*index).as_deref().unwrap_or_default();

                let (input, completion) = if search_term.is_empty() {
                    (history_entry, "")
                } else {
                    (search_term.as_str(), &history_entry[search_term.len()..])
                };

                Line::from(vec![
                    input.white(),
                    completion.cyan()
                ])
            }
        }
    }

    pub fn cursor_position(&self) -> usize {
        match &self.state {
            InputState::Typing { buffer: _, cursor_position } => {
                *cursor_position
            },
            InputState::HistorySearch { search_term, index } => {
                if search_term == "" {
                    self.history.get(*index).as_deref().unwrap_or_default().len()
                } else {
                    search_term.chars().count()
                }
            }
        }
    }

    pub fn type_string(&mut self, stuff: String) {
        match &mut self.state {
            InputState::Typing { buffer, cursor_position } => {
                *buffer = insert_string(
                    std::mem::take(buffer),
                    &stuff,
                    *cursor_position);

                *cursor_position += stuff.chars().count();
            },
            _ => {},
        }
    }

    pub fn cancel_history_search(&mut self) {
        if let InputState::HistorySearch { search_term, index } = &self.state {
            let buffer = if search_term.is_empty() {
                self.history.get(*index).clone().unwrap_or_default()
            } else {
                search_term.to_string()
            };

            self.state = InputState::typing_from_buffer(buffer);
        }
    }

    pub fn left(&mut self) {
        self.cancel_history_search();

        if let InputState::Typing { buffer: _, cursor_position } = &mut self.state {
            *cursor_position = cursor_position.saturating_sub(1);
        }
    }

    pub fn right(&mut self) {
        match &mut self.state {
            InputState::Typing { buffer, cursor_position } => {
                if cursor_position < &mut buffer.chars().count() {
                    *cursor_position += 1;
                }
            },
            InputState::HistorySearch { search_term: _, index } => {
                let buffer = self.history.get(*index).clone().unwrap_or_default();
                self.state = InputState::typing_from_buffer(buffer);
            },
        }
    }

    pub fn home(&mut self) {
        self.cancel_history_search();

        if let InputState::Typing { buffer: _, cursor_position } = &mut self.state {
            *cursor_position = 0;
        }
    }

    pub fn end(&mut self) {
        match &mut self.state {
            InputState::Typing { buffer, cursor_position } => {
                *cursor_position = buffer.chars().count();
            },
            InputState::HistorySearch { search_term: _, index } => {
                let buffer = self.history.get(*index).clone().unwrap_or_default();
                self.state = InputState::typing_from_buffer(buffer);
            },
        }
    }

    pub fn backspace(&mut self) {
        self.cancel_history_search();

        if let InputState::Typing { buffer, cursor_position } = &mut self.state {
            if *cursor_position > 0 {
                *buffer = del_from_string(
                    std::mem::take(buffer),
                    *cursor_position - 1);
                *cursor_position = cursor_position.saturating_sub(1);
            }
        }
    }

    pub fn delete(&mut self) {
        self.cancel_history_search();

        if let InputState::Typing { buffer, cursor_position } = &mut self.state {
            *buffer = del_from_string(
                std::mem::take(buffer),
                *cursor_position);
        }
    }

    pub fn up(&mut self) {
        self.state = match &mut self.state {
            InputState::Typing { buffer, cursor_position: _ } => {
                if self.history.is_empty() {
                    self.state.clone()
                } else if let Some(index) = self.history.find_forwards(
                    |x| x.starts_with(buffer.as_str()),
                    self.history.size() - 1) {
                    InputState::HistorySearch { search_term: buffer.clone(), index }
                } else {
                    self.state.clone()
                }
            },
            InputState::HistorySearch { search_term, index } => {
                if *index == 0 {
                    self.state.clone()
                } else if let Some(find_index) = self.history.find_forwards(
                    |x| x.starts_with(search_term.as_str()),
                    *index - 1) {
                    InputState::HistorySearch { search_term: search_term.clone(), index: find_index }
                } else {
                    self.state.clone()
                }
            },
        };
    }

    pub fn down(&mut self) {
        self.state = match &mut self.state {
            InputState::Typing { buffer: _, cursor_position: _ } => {
                self.state.clone()
            },
            InputState::HistorySearch { search_term, index } => {
                if *index == self.history.size() {
                    InputState::Typing { buffer: search_term.clone(), cursor_position: search_term.chars().count() }
                } else if let Some(find_index) = self.history.find_backwards(
                    |x| x.starts_with(search_term.as_str()),
                    *index + 1) {
                    InputState::HistorySearch { search_term: search_term.clone(), index: find_index }
                } else {
                    InputState::typing_from_buffer(search_term.clone())
                }
            },
        };
    }

    pub fn cancel(&mut self) {
        self.state = match &mut self.state {
            InputState::Typing { buffer: _, cursor_position: _ } => {
                InputState::empty_typing()
            },
            InputState::HistorySearch { search_term: term, index: _ } => {
                InputState::typing_from_buffer(term.clone())
            }
        };
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