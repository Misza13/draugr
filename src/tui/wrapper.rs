use ansi_to_tui::IntoText;
use tokio::sync::mpsc::Sender;
use anyhow::{Context, Result};

use ratatui::prelude::*;

use crate::tui::*;

pub struct TuiWrapper {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    tx: Sender<TuiEvent>,

    layout: LayoutElement,
    active_pane: usize,
}

impl TuiWrapper {
    pub fn new(terminal: Terminal<CrosstermBackend<Stdout>>, tx: Sender<TuiEvent>) -> TuiWrapper {
        TuiWrapper { terminal, tx, layout: TuiWrapper::default_layout(), active_pane: 1 }
    }

    fn default_layout() -> LayoutElement {
        LayoutElement::VerticalStack {
            children: vec![
                LayoutElement::Pane(LayoutPane::ScrollPane {
                    id: Some(1),
                    pane: ScrollPane::new(2000),
                }),
                LayoutElement::Pane(LayoutPane::InputPane(
                    InputPane::new()
                ))
            ],
            constraints: vec![
                Constraint::Max(9999),
                Constraint::Min(2),
            ],
        }
    }

    pub fn render_ui(&mut self) -> Result<()> {
        self.terminal.draw(|frame| {
            let area = frame.size();

            self.layout.render(frame, area, self.active_pane);
        }).context("Draw to terminal")?;

        Ok(())
    }

    pub async fn process_input(&mut self, event: Event) -> Result<bool> {
        if let event::Event::Key(key) = event {
            if key.kind == KeyEventKind::Press {
                match (key.modifiers, key.code) {
                    /* Alt+q = Exit program */
                    (KeyModifiers::ALT, KeyCode::Char('q')) => {
                        self.tx.send(TuiEvent::Quit).await?;

                        return Ok(true);
                    },

                    /* Enter = submit input */
                    (KeyModifiers::NONE, KeyCode::Enter) => {
                        let data = self.input().get_and_submit();
                        self.tx.send(TuiEvent::Send(data)).await
                            .context("Submit user input")?;
                    },
                    /* Alt+Enter = submit secret (e.g. password) */
                    (KeyModifiers::ALT, KeyCode::Enter) => {
                        let data = self.input().get_and_clear();
                        self.tx.send(TuiEvent::SendSecret(data)).await
                            .context("Submit secret user input")?;
                    },

                    /* Lowercase characters */
                    (KeyModifiers::NONE, KeyCode::Char(ch)) => {
                        self.input().type_string(ch.to_string());
                    },
                    /* Uppercase characters */
                    (KeyModifiers::SHIFT, KeyCode::Char(ch)) => {
                        self.input().type_string(ch.to_ascii_uppercase().to_string());
                    },

                    /* Backspace */
                    (KeyModifiers::NONE, KeyCode::Backspace) => { self.input().backspace(); },
                    /* Delete */
                    (KeyModifiers::NONE, KeyCode::Delete) => { self.input().delete(); },

                    /* Navigation */
                    (KeyModifiers::NONE, KeyCode::Right) => { self.input().right(); },
                    (KeyModifiers::NONE, KeyCode::Left) => { self.input().left(); },
                    (KeyModifiers::NONE, KeyCode::Home) => { self.input().home(); },
                    (KeyModifiers::NONE, KeyCode::End) => { self.input().end(); },
                    (KeyModifiers::NONE, KeyCode::Up) => { self.input().up() }
                    (KeyModifiers::NONE, KeyCode::Down) => { self.input().down() }
                    (KeyModifiers::NONE, KeyCode::PageUp) => { self.active_pane().page_up(); }
                    (KeyModifiers::NONE, KeyCode::PageDown) => { self.active_pane().page_down(); }

                    /* Escape = cancel completion suggestions */
                    (KeyModifiers::NONE, KeyCode::Esc) => { self.input().cancel(); }

                    /* Unhandled */
                    _ => {
                        self.default_pane().push(format!("Unhandled key: {:?}", key).light_yellow().into());
                    },
                }
            }
        }

        Ok(false)
    }

    pub fn process_request(&mut self, recv: TuiRequest) -> Result<()> {
        match recv {
            TuiRequest::Print(data, _) => {
                let line = data.into_text()
                    .context("Parse ANSI color codes")?
                    .lines;
                self.default_pane().append(line);
            },
            TuiRequest::PrintUserInput(data, _) => {
                self.default_pane().push(data.light_cyan().bold().into());
            },
            TuiRequest::PrintInfo(data, _) => {
                for line in data.split('\n') {
                    self.default_pane().push(format!("[INFO] {line}").light_green().into());
                }
            },
            TuiRequest::PrintWarning(data, _) => {
                for line in data.split('\n') {
                    self.default_pane().push(format!("[WARN] {line}").light_yellow().into());
                }
            },
            TuiRequest::PrintError(data, _) => {
                for line in data.split('\n') {
                    self.default_pane().push(format!("[ERR] {line}").light_red().into());
                }
            },
            TuiRequest::SetLayout(layout) => {
                self.layout = layout; /* TODO: copy over the buffers */
            },
        }

        Ok(())
    }

    fn input(&mut self) -> &mut InputPane {
        if let Some(input) = self.layout.input() {
            input
        } else {
            panic!("No input!");
        }
    }

    fn default_pane(&mut self) -> &mut ScrollPane {
        self.layout.pane(1)
            .expect("There should be a pane with id = 1")
    }

    fn active_pane(&mut self) -> &mut ScrollPane {
        self.layout.pane(self.active_pane)
            .expect("There should be an active pane")
    }
}