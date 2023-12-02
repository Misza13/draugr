use std::io::stdout;
use ansi_to_tui::IntoText;
use tokio::sync::mpsc::{channel, Sender, Receiver};
use anyhow::Context;
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
    widgets::*,
};

pub enum TuiRequest {
    Print(String, usize),
    PrintUserInput(String, usize),
    PrintWarning(String, usize),
    PrintError(String, usize),
}

pub enum TuiEvent {
    Send(String),
    Quit,
}

pub fn create_tui() -> Result<(Sender<TuiRequest>, Receiver<TuiEvent>), anyhow::Error> {
    let (req_tx, mut req_rx) = channel(256);
    let (ev_tx, ev_rx) = channel(256);

    stdout().execute(EnterAlternateScreen)?;
    enable_raw_mode()?;

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.clear()?;

    tokio::spawn(async move {
        let mut buffer: Vec<Line> = vec!["Welcome to Draugr! (press 'Alt+q' to quit)\n".into()];

        let mut input_buffer = Box::new("".to_string());
        let mut input_index = 0_usize;

        loop {
            terminal.draw(|frame| {
                let area = frame.size();

                let chunks = Layout::default()
                    .constraints([
                        Constraint::Length(area.height - 2),
                        Constraint::Length(2)
                    ])
                    .split(area);

                let max_lines = chunks[0].height as usize;
                let last: Vec<Line> = if buffer.len() > max_lines {
                    buffer.iter().skip(buffer.len() - max_lines).cloned().collect()
                } else {
                    buffer.to_vec()
                };

                let wraps: u16 = last.iter().map(|l| { (l.width().saturating_sub(1) as u16) / chunks[0].width }).sum();

                frame.render_widget(
                    Paragraph::new(Text::from(last))
                        .wrap(Wrap { trim: false })
                        .scroll((wraps, 0)),
                    chunks[0],
                );

                frame.render_widget(
                    Paragraph::new(input_buffer.as_str())
                        .block(Block::default().borders(Borders::TOP)),
                    chunks[1]
                );

                frame.set_cursor(
                    chunks[1].left() + input_index as u16,
                    chunks[1].bottom());
            }).context("Draw to terminal")?;

            if let Ok(true) = event::poll(std::time::Duration::from_millis(20)) {
                if let event::Event::Key(key) = event::read().context("Read key event")? {
                    if key.kind == KeyEventKind::Press {
                        if key.modifiers  == KeyModifiers::ALT {
                            match key.code {
                                KeyCode::Char('q') => {
                                    ev_tx.send(TuiEvent::Quit).await?;
                                    break;
                                },
                                KeyCode::Char('w') => {
                                    ev_tx.send(TuiEvent::Quit).await?;
                                },
                                _ => todo!()
                            }
                        } else if key.modifiers == KeyModifiers::SHIFT {
                            match key.code {
                                KeyCode::Char(ch) => {
                                    input_buffer.insert(input_index, ch.to_ascii_uppercase());
                                    input_index += 1;
                                },
                                _ => {
                                    buffer.push(format!("Unhandled key: {:?}", key).light_yellow().into());
                                }
                            }
                        } else if key.modifiers == KeyModifiers::NONE {
                            match key.code {
                                KeyCode::Char(ch) => {
                                    input_buffer.insert(input_index, ch);
                                    input_index += 1;
                                },
                                KeyCode::Backspace => {
                                    if input_index > 0 {
                                        input_buffer.remove(input_index - 1);
                                        input_index -= 1;
                                    }
                                },
                                KeyCode::Right => {
                                    if input_index < input_buffer.len() {
                                        input_index += 1;
                                    }
                                },
                                KeyCode::Left => {
                                    input_index = input_index.saturating_sub(1);
                                },
                                KeyCode::Home => {
                                    input_index = 0;
                                },
                                KeyCode::End => {
                                    input_index = input_buffer.len();
                                },
                                KeyCode::Enter => {
                                    ev_tx.send(TuiEvent::Send(input_buffer.to_string())).await?;
                                    input_buffer.clear();
                                    input_index = 0;
                                },
                                _ => {
                                    buffer.push(format!("Unhandled key: {:?}", key).light_yellow().into());
                                }
                            }
                        }
                    }
                }
            }

            if let Ok(recv) = req_rx.try_recv() {
                match recv {
                    TuiRequest::Print(data, _) => {
                        let mut line = data.into_text()
                            .context("Parse ANSI color codes")?
                            .lines;
                        buffer.append(&mut line);
                    },
                    TuiRequest::PrintUserInput(data, _) => {
                        buffer.push(data.light_blue().bold().into());
                    },
                    TuiRequest::PrintWarning(data, _) => {
                        for line in data.split('\n') {
                            buffer.push(line.to_string().light_yellow().into());
                        }
                    },
                    TuiRequest::PrintError(data, _) => {
                        for line in data.split('\n') {
                            buffer.push(line.to_string().light_red().into());
                        }
                    },
                }
            }

            tokio::task::yield_now().await;
        }

        stdout().execute(LeaveAlternateScreen)?;
        disable_raw_mode()?;

        Ok::<(), anyhow::Error>(())
    });

    Ok((req_tx, ev_rx))
}
