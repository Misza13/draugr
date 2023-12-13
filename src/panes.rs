use ratatui::{
    prelude::*,
    widgets::{*, block::*},
};

use crate::ring::RingBuffer;

pub struct ScrollPane<'a> {
    buffer: RingBuffer<Line<'a>>,

    scroll_offset: usize,

    last_seen_area: Rect,
}

impl<'a> ScrollPane<'a> {
    pub fn new(capacity: usize) -> ScrollPane<'a> {
        ScrollPane {
            buffer: RingBuffer::new(capacity),
            scroll_offset: 0,
            last_seen_area: Rect::new(0, 0, 1, 1),
        }
    }

    pub fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let mut last: Vec<Line> = self.buffer
            .iter_from_back()
            .skip(self.scroll_offset)
            .take(area.height as usize - 1 /* -1 for top bar */)
            .collect();
        last.reverse();

        let wraps: u16 = last.iter().map(|l| { (l.width().saturating_sub(1) as u16) / area.width }).sum();

        frame.render_widget(
            Paragraph::new(Text::from(last))
                .block(Block::default()
                    .title(Title::from(vec!["[".yellow(), "1".dark_gray(), "]".yellow()])
                    .alignment(Alignment::Center))
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(Color::Yellow)))
                .wrap(Wrap { trim: false })
                .scroll((wraps, 0)),
            area,
        );

        self.last_seen_area = area;
    }

    pub fn push(&mut self, line: Line<'a>) {
        self.buffer.push_back(line);
        if self.scroll_offset > 0 {
            self.scroll_offset = (self.scroll_offset + 1)
                .min(self.buffer.size() - self.last_seen_area.height as usize);
        }
    }

    pub fn append(&mut self, lines: Vec<Line<'a>>) {
        for line in lines {
            self.push(line);
        }
    }

    pub fn page_up(&mut self) {
        self.scroll_offset = (self.scroll_offset + self.last_seen_area.height as usize / 2)
            .min(self.buffer.size() - self.last_seen_area.height as usize);
    }

    pub fn page_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(self.last_seen_area.height as usize / 2);
    }

}