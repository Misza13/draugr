use std::io::{stdout, Stdout};
use ansi_to_tui::IntoText;
use tokio::sync::mpsc::{channel, Sender, Receiver};
use anyhow::{Context, Result, Error};
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

            buffer: vec!["Welcome to Draugr! (press 'Alt+q' to quit)\n".into()],
            input_buffer: String::from(""),
            input_index: 0,
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

    buffer: Vec<Line<'a>>,
    input_buffer: String,
    input_index: usize,
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
                Paragraph::new(self.input_buffer.as_str())
                    .block(Block::default().borders(Borders::TOP)
                    .border_style(Style::default().fg(Color::Yellow))),
                chunks[1]
            );

            frame.set_cursor(
                chunks[1].left() + self.input_index as u16,
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
                            self.tx.send(TuiEvent::Send(self.input_buffer.to_string())).await?;
                            self.input_buffer.clear();
                            self.input_index = 0;
                        },
                        /* Alt+Enter = submit secret (e.g. password) */
                        (KeyModifiers::ALT, KeyCode::Enter) => {
                            self.tx.send(TuiEvent::SendSecret(self.input_buffer.to_string())).await?;
                            self.input_buffer.clear();
                            self.input_index = 0;
                        },

                        /* Lowercase characters */
                        (KeyModifiers::NONE, KeyCode::Char(ch)) => {
                            self.input_buffer = insert_string(
                                &self.input_buffer,
                                ch.to_string(),
                                self.input_index)
                                .context("Insert character to input string")?;
                            self.input_index += 1;
                        },
                        /* Uppercase characters */
                        (KeyModifiers::SHIFT, KeyCode::Char(ch)) => {
                            self.input_buffer = insert_string(
                                &self.input_buffer,
                                ch.to_uppercase().to_string(),
                                self.input_index)
                                .context("Insert character to input string")?;
                            self.input_index += ch.to_ascii_uppercase().to_string().chars().count();
                        },

                        /* Backspace */
                        (KeyModifiers::NONE, KeyCode::Backspace) => {
                            if self.input_index > 0 {
                                self.input_buffer = del_from_string(&self.input_buffer, self.input_index - 1)
                                    .context("Remove character from input string (Backspace)")?;
                                self.input_index = self.input_index.saturating_sub(1);
                            }
                        },
                        /* Delete */
                        (KeyModifiers::NONE, KeyCode::Delete) => {
                            self.input_buffer = del_from_string(&self.input_buffer, self.input_index)
                                .context("Remove character from input string (Delete)")?;
                        },

                        /* Navigation */
                        (KeyModifiers::NONE, KeyCode::Right) => {
                            if self.input_index < self.input_buffer.chars().count() {
                                self.input_index += 1;
                            }
                        },
                        (KeyModifiers::NONE, KeyCode::Left) => {
                            self.input_index = self.input_index.saturating_sub(1);
                        },
                        (KeyModifiers::NONE, KeyCode::Home) => {
                            self.input_index = 0;
                        },
                        (KeyModifiers::NONE, KeyCode::End) => {
                            self.input_index = self.input_buffer.chars().count();
                        },

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

fn insert_string(source: &String, what: String, position: usize) -> Result<String> {
    if position > source.chars().count() {
        return Err(Error::msg("Insert position out of bounds"));
    }

    let byte_position = source.char_indices().nth(position).map_or(source.len(), |(idx, _)| idx);

    let (before, after) = source.split_at(byte_position);

    Ok(format!("{before}{what}{after}"))
}

fn del_from_string(source: &String, position: usize) -> Result<String> {
    let source_len = source.chars().count();

    if position > source_len {
        return Err(Error::msg("Deletion position out of bounds"));
    } else if position == source_len {
        return Ok(source.to_owned());
    }

    let byte_position = source.char_indices().nth(position).map_or(source.len(), |(idx, _)| idx);

    let (before, after) = source.split_at(byte_position);

    Ok(format!("{before}{}", after.chars().skip(1).collect::<String>()))
}