use std::{io::{stdout, Stdout}, any::type_name};
use ansi_to_tui::IntoText;
use rhai::{Map, Dynamic};
use tokio::sync::mpsc::{channel, Sender, Receiver};
use anyhow::{Context, Result, bail, anyhow};
use crossterm::{
    event::{self, KeyCode, KeyEventKind, KeyModifiers},
    terminal::{
        disable_raw_mode, enable_raw_mode, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
    ExecutableCommand,
};
use ratatui::prelude::*;

use crate::{
    input::*,
    panes::*
};

pub enum TuiRequest {
    Print(String, usize),
    PrintUserInput(String, usize),
    PrintInfo(String, usize),
    PrintWarning(String, usize),
    PrintError(String, usize),
    SetLayout(LayoutElement),
}

pub enum TuiEvent {
    Send(String),
    SendSecret(String),
    Quit,
}

pub async fn create_tui() -> Result<(Sender<TuiRequest>, Receiver<TuiEvent>), anyhow::Error> {
    let (req_tx, req_rx) = channel(256);
    let (ev_tx, ev_rx) = channel(256);

    stdout().execute(EnterAlternateScreen)?;
    enable_raw_mode()?;

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.clear()?;

    tokio::spawn(async move {
        let layout = LayoutElement::VerticalStack {
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
        };

        let mut tui = TuiWrapper {
            terminal,
            rx: req_rx,
            tx: ev_tx,

            layout,
            active_pane: 1,
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

    req_tx.send(TuiRequest::Print("Welcome to Draugr! (press 'Alt+q' to quit)\n".into(), 1)).await
        .context("Send welcome message")?;

    Ok((req_tx, ev_rx))
}


struct TuiWrapper {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    rx: Receiver<TuiRequest>,
    tx: Sender<TuiEvent>,

    layout: LayoutElement,
    active_pane: usize,
}

impl TuiWrapper {
    fn render_ui(&mut self) -> Result<()> {
        self.terminal.draw(|frame| {
            let area = frame.size();

            self.layout.render(frame, area, self.active_pane);
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
                    self.layout = layout;
                },
            }
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

pub enum LayoutElement {
    VerticalStack {
        children: Vec<LayoutElement>,
        constraints: Vec<Constraint>,
    },
    HorizontalStack {
        children: Vec<LayoutElement>,
        constraints: Vec<Constraint>,
    },
    Pane(LayoutPane),
}

pub enum LayoutPane {
    ScrollPane { id: Option<usize>, pane: ScrollPane, },
    // StaticPane { id: Option<usize>, pane: StaticPane, },
    InputPane(InputPane),
}

impl LayoutElement {
    pub fn from(layout: Map) -> Result<LayoutElement> {
        let element_type: String = layout.get("type")
            .convert("layout element type")?;

        match element_type.as_str() {
            "vstack" => {
                let (children, constraints) = parse_container(layout)
                    .context("Parse vstack container")?;

                Ok(LayoutElement::VerticalStack { children, constraints })
            },
            "hstack" => {
                let (children, constraints) = parse_container(layout)
                    .context("Parse hstack container")?;

                Ok(LayoutElement::HorizontalStack { children, constraints })
            },
            "scroll" => {
                let id = if let Some(id) = layout.get("id") {
                    Some(id.as_int()
                        .map_err(|err| anyhow!(err))
                        .context("Parse pane id as int")? as usize)
                } else {
                    None
                };

                Ok(LayoutElement::Pane(LayoutPane::ScrollPane {
                    id,
                    pane: ScrollPane::new(1000)
                }))
            },
            /*"static" => {
                let id = if let Some(id) = layout.get("id") {
                    Some(id.as_int()
                        .map_err(|err| anyhow!(err))
                        .context("Parse pane id as int")? as usize)
                } else {
                    None
                };

                Ok(LayoutElement::Pane(LayoutPane::StaticPane {
                    id,
                    pane: StaticPane {},
                }))
            },*/
            "input" => {
                Ok(LayoutElement::Pane(LayoutPane::InputPane(InputPane::new())))
            }
            _ => {
                bail!("Invalid layout element type: {element_type}");
            },
        }
    }

    pub fn render(&mut self, frame: &mut Frame<'_>, area: Rect, active_pane: usize) {
        match self {
            LayoutElement::VerticalStack { children, constraints } => {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints(constraints.clone())
                    .split(area);

                for (i, child) in children.iter_mut().enumerate() {
                    child.render(frame, chunks[i], active_pane);
                }
            },
            LayoutElement::HorizontalStack { children, constraints } => {
                let chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints(constraints.clone())
                    .split(area);

                for (i, child) in children.iter_mut().enumerate() {
                    child.render(frame, chunks[i], active_pane);
                }
            },
            LayoutElement::Pane(pane) => match pane {
                LayoutPane::ScrollPane { id, pane } => {
                    pane.render(frame, area, *id, *id == Some(active_pane));
                },
                LayoutPane::InputPane(input_pane) => {
                    input_pane.render(frame, area);
                },
                // LayoutPane::StaticPane { id: _, pane: _ } => { /* TODO */},
            },
        }

    }

    pub fn pane(&mut self, pane_id: usize) -> Option<&mut ScrollPane> {
        match self {
            LayoutElement::HorizontalStack { children, constraints: _ } => {
                children.iter_mut().find_map(|child| child.pane(pane_id))
            },
            LayoutElement::VerticalStack { children, constraints: _ } => {
                children.iter_mut().find_map(|child| child.pane(pane_id))
            },
            LayoutElement::Pane(LayoutPane::ScrollPane { id: Some(id), pane }) if pane_id == *id => {
                Some(pane)
            },
            _ => { None },
        }
    }

    pub fn input(&mut self) -> Option<&mut InputPane> {
        match self {
            LayoutElement::HorizontalStack { children, constraints: _ } => {
                children.iter_mut().find_map(|child| child.input())
            },
            LayoutElement::VerticalStack { children, constraints: _ } => {
                children.iter_mut().find_map(|child| child.input())
            },
            LayoutElement::Pane(LayoutPane::InputPane(input_pane)) => {
                Some(input_pane)
            },
            _ => { None },
        }
    }
}

fn parse_container(layout: Map) -> Result<(Vec<LayoutElement>, Vec<Constraint>)> {
    let children = get_array_property(
        &layout,
        "children",
        create_layout_element)
        .context("Parse container's children")?;

    let constraints = get_array_property(
        &layout,
        "constraints",
        create_constraint)
        .context("Parse container's constraints")?;

    Ok((children, constraints))
}

fn get_array_property<T>(layout: &Map, property_name: &str, mapper: impl Fn(&Dynamic) -> Result<T>) -> Result<Vec<T>> {
    let items: Vec<_> = layout.get(property_name)
        .convert(format!("property \"{property_name}\"").as_str())?;

    let mut result = vec![];

    for item in items {
        let item = mapper(&item)
            .context(format!("Parse item: {item}"))?;

        result.push(item);
    }

    Ok(result)
}

fn create_layout_element(item: &Dynamic) -> Result<LayoutElement> {
    let map = item.clone().try_cast::<Map>()
        .context("Cast layout element to map")?;

    LayoutElement::from(map)
        .context("Create layout element")
}


fn create_constraint(item: &Dynamic) -> Result<Constraint> {
    let constraint: Vec<_> = Some(item)
        .convert("constraint")?;

    let constraint_type: String = constraint.get(0)
        .convert("constraint type")?;

    match constraint_type.as_str() {
        "max" => {
            let max_value: i64 = constraint.get(1)
                .convert("constraint max value")?;

            Ok(Constraint::Max(max_value as u16))
        },
        "min" => {
            let min_value: i64 = constraint.get(1)
                .convert("constraint min value")?;

            Ok(Constraint::Min(min_value as u16))
        }
        "percentage" => {
            let prc_value: i64 = constraint.get(1)
                .convert("constraint percentage value")?;

            Ok(Constraint::Percentage(prc_value as u16))
        }
        _ => {
            bail!("Invalid constraint type: {}", constraint_type);
        }
    }
}

trait DynamicExt {
    fn convert<T: Clone + 'static>(self, what: &str) -> Result<T>;
}

impl DynamicExt for Option<&Dynamic> {
    fn convert<T: Clone + 'static>(self, what: &str) -> Result<T> {
        self
            .context(format!("Get {what}"))?
            .clone()
            .try_cast::<T>()
            .context(format!("Get {what} as {}", type_name::<T>()))
    }
}