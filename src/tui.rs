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
use ratatui::prelude::*;

use crate::{input::InputPane, panes::ScrollPane};

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

            input: InputPane::new(),

            pane1: ScrollPane::new(10000),
        };

        tui.pane1.push("Welcome to Draugr! (press 'Alt+q' to quit)\n".into());

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

    input: InputPane,

    pane1: ScrollPane<'a>,
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

            self.pane1.render(frame, chunks[0]);

            self.input.render(frame, chunks[1]);
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
                            self.tx.send(TuiEvent::Send(self.input.get_and_submit())).await
                                .context("Submit user input")?;
                        },
                        /* Alt+Enter = submit secret (e.g. password) */
                        (KeyModifiers::ALT, KeyCode::Enter) => {
                            self.tx.send(TuiEvent::SendSecret(self.input.get_and_clear())).await
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
                        (KeyModifiers::NONE, KeyCode::Up) => { self.input.up() }
                        (KeyModifiers::NONE, KeyCode::Down) => { self.input.down() }

                        /* Escape = cancel completion suggestions */
                        (KeyModifiers::NONE, KeyCode::Esc) => { self.input.cancel(); }

                        /* Unhandled */
                        _ => {
                            self.pane1.push(format!("Unhandled key: {:?}", key).light_yellow().into());
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
                    let line = data.into_text()
                        .context("Parse ANSI color codes")?
                        .lines;
                    self.pane1.append(line);
                },
                TuiRequest::PrintUserInput(data, _) => {
                    self.pane1.push(data.light_cyan().bold().into());
                },
                TuiRequest::PrintInfo(data, _) => {
                    for line in data.split('\n') {
                        self.pane1.push(format!("[INFO] {line}").light_green().into());
                    }
                },
                TuiRequest::PrintWarning(data, _) => {
                    for line in data.split('\n') {
                        self.pane1.push(format!("[WARN] {line}").light_yellow().into());
                    }
                },
                TuiRequest::PrintError(data, _) => {
                    for line in data.split('\n') {
                        self.pane1.push(format!("[ERR] {line}").light_red().into());
                    }
                },
            }
        }

        Ok(())
    }
}