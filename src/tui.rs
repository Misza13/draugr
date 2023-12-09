use std::io::{stdout, Stdout};
use ansi_to_tui::IntoText;
use tokio::sync::mpsc::{channel, Sender, Receiver};
use anyhow::{Context, Result};
use crossterm::{
    event::{self, KeyCode, KeyEventKind, KeyModifiers},
    terminal::{
        disable_raw_mode, enable_raw_mode, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
    ExecutableCommand,
};
use ratatui::{
    prelude::*,
    widgets::{*, block::*},
};
use crate::input::InputLine;

pub enum TuiRequest {
    Print(String, usize),
    PrintUserInput(String, usize),
    PrintInfo(String, usize),
    PrintWarning(String, usize),
    PrintError(String, usize),
}

pub enum TuiEvent {
    Send(String),
    SendSecret(String),
    Quit,
}

pub fn create_tui() -> Result<(Sender<TuiRequest>, Receiver<TuiEvent>), anyhow::Error> {
    let (req_tx, req_rx) = channel(256);
    let (ev_tx, ev_rx) = channel(256);

    stdout().execute(EnterAlternateScreen)?;
    enable_raw_mode()?;

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.clear()?;

    tokio::spawn(async move {
        let mut tui = TuiWrapper {
            terminal,
            rx: req_rx,
            tx: ev_tx,

            input: InputLine::new(),

            buffer: vec!["Welcome to Draugr! (press 'Alt+q' to quit)\n".into()],
        };

        loop {
            tui.render_ui()
                .context("Render UI")?;

            if tui.process_input().await
                .context("Process input")? { /* Shutdown signal */ break; }

            tui.process_request()
                .context("Process input")?;

            tokio::task::yield_now().await;
        }

        stdout().execute(LeaveAlternateScreen)?;
        disable_raw_mode()?;

        Ok::<(), anyhow::Error>(())
    });

    Ok((req_tx, ev_rx))
}


struct TuiWrapper<'a> {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    rx: Receiver<TuiRequest>,
    tx: Sender<TuiEvent>,

    input: InputLine,

    buffer: Vec<Line<'a>>,
}

impl<'a> TuiWrapper<'a> {
    fn render_ui(&mut self) -> Result<()> {
        self.terminal.draw(|frame| {
            let area = frame.size();

            let chunks = Layout::default()
                .constraints([
                    Constraint::Max(9999),
                    Constraint::Length(2),
                ])
                .split(area);

            let max_lines = chunks[0].height as usize;
            let last: Vec<Line> = if self.buffer.len() > max_lines {
                self.buffer.iter().skip(self.buffer.len() - max_lines).cloned().collect()
            } else {
                self.buffer.to_vec()
            };

            let wraps: u16 = last.iter().map(|l| { (l.width().saturating_sub(1) as u16) / chunks[0].width }).sum();

            frame.render_widget(
                Paragraph::new(Text::from(last))
                    .block(Block::default()
                        .title(Title::from(vec!["[".yellow(), "1".dark_gray(), "]".yellow()])
                        .alignment(Alignment::Center))
                        .borders(Borders::TOP)
                        .border_style(Style::default().fg(Color::Yellow)))
                    .wrap(Wrap { trim: false })
                    .scroll((wraps, 0)),
                chunks[0],
            );

            frame.render_widget(
                Paragraph::new(self.input.as_line())
                    .block(Block::default().borders(Borders::TOP)
                    .border_style(Style::default().fg(Color::Yellow))),
                chunks[1]
            );

            frame.set_cursor(
                chunks[1].left() + self.input.cursor_position() as u16,
                chunks[1].bottom());
        }).context("Draw to terminal")?;

        Ok(())
    }

    async fn process_input(&mut self) -> Result<bool> {
        if let Ok(true) = event::poll(std::time::Duration::from_millis(20)) {
            if let event::Event::Key(key) = event::read().context("Read key event")? {
                if key.kind == KeyEventKind::Press {
                    match (key.modifiers, key.code) {
                        /* Alt+q = Exit program */
                        (KeyModifiers::ALT, KeyCode::Char('q')) => {
                            self.tx.send(TuiEvent::Quit).await?;

                            return Ok(true);
                        },

                        /* Enter = submit input */
                        (KeyModifiers::NONE, KeyCode::Enter) => {
                            self.tx.send(TuiEvent::Send(self.input.get_buffer_and_clear())).await
                                .context("Submit user input")?;
                        },
                        /* Alt+Enter = submit secret (e.g. password) */
                        (KeyModifiers::ALT, KeyCode::Enter) => {
                            self.tx.send(TuiEvent::SendSecret(self.input.get_buffer_and_clear())).await
                                .context("Submit secret user input")?;
                        },

                        /* Lowercase characters */
                        (KeyModifiers::NONE, KeyCode::Char(ch)) => {
                            self.input.type_string(ch.to_string());
                        },
                        /* Uppercase characters */
                        (KeyModifiers::SHIFT, KeyCode::Char(ch)) => {
                            self.input.type_string(ch.to_ascii_uppercase().to_string());
                        },

                        /* Backspace */
                        (KeyModifiers::NONE, KeyCode::Backspace) => { self.input.backspace(); },
                        /* Delete */
                        (KeyModifiers::NONE, KeyCode::Delete) => { self.input.delete(); },

                        /* Navigation */
                        (KeyModifiers::NONE, KeyCode::Right) => { self.input.right(); },
                        (KeyModifiers::NONE, KeyCode::Left) => { self.input.left(); },
                        (KeyModifiers::NONE, KeyCode::Home) => { self.input.home(); },
                        (KeyModifiers::NONE, KeyCode::End) => { self.input.end(); },

                        /* Unhandled */
                        _ => {
                            self.buffer.push(format!("Unhandled key: {:?}", key).light_yellow().into());
                        },
                    }
                }
            }
        }

        Ok(false)
    }

    fn process_request(&mut self) -> Result<()> {
        if let Ok(recv) = self.rx.try_recv() {
            match recv {
                TuiRequest::Print(data, _) => {
                    let mut line = data.into_text()
                        .context("Parse ANSI color codes")?
                        .lines;
                    self.buffer.append(&mut line);
                },
                TuiRequest::PrintUserInput(data, _) => {
                    self.buffer.push(data.light_cyan().bold().into());
                },
                TuiRequest::PrintInfo(data, _) => {
                    for line in data.split('\n') {
                        self.buffer.push(format!("[INFO] {line}").light_green().into());
                    }
                },
                TuiRequest::PrintWarning(data, _) => {
                    for line in data.split('\n') {
                        self.buffer.push(format!("[WARN] {line}").light_yellow().into());
                    }
                },
                TuiRequest::PrintError(data, _) => {
                    for line in data.split('\n') {
                        self.buffer.push(format!("[ERR] {line}").light_red().into());
                    }
                },
            }
        }

        Ok(())
    }
}