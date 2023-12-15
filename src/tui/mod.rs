mod input;
mod layout;
mod panes;
mod wrapper;

use std::io::{stdout, Stdout};
use tokio::sync::mpsc::{channel, Sender, Receiver};
use anyhow::{Context, Result};
use crossterm::{
    event::{self, KeyCode, KeyEventKind, KeyModifiers, EventStream, Event},
    terminal::{
        disable_raw_mode, enable_raw_mode, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
    ExecutableCommand,
};
use tokio_stream::StreamExt;
use ratatui::prelude::*;

use input::*;
use layout::*;
use panes::*;
use wrapper::*;

pub use layout::LayoutElement;

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
    let (req_tx, mut req_rx) = channel(256);
    let (ev_tx, ev_rx) = channel(256);

    let mut terminal = init_terminal()
        .context("Initialize terminal")?;

    install_panic_hook();

    terminal.clear()?;

    tokio::spawn(async move {
        let mut tui = TuiWrapper::new(terminal, ev_tx);

        let mut event_stream = EventStream::new();

        loop {
            tui.render_ui()
                .context("Render UI")?;

            tokio::select! {
                event = event_stream.next() =>
                    match event {
                        Some(Ok(event)) => {
                            tui.process_input(event).await
                                .context("Process input event")?;
                        },
                        None => break,
                        _ => {},
                    },

                request = req_rx.recv() =>
                    match request {
                        Some(request) => {
                            tui.process_request(request)
                                .context("Process input request")?;
                        },
                        None => break,
                    }
            }

            tokio::task::yield_now().await;
        }

        restore_terminal()
            .context("Restore terminal")?;

        Ok::<(), anyhow::Error>(())
    });

    req_tx.send(TuiRequest::Print("Welcome to Draugr! (press 'Alt+q' to quit)\n".into(), 1)).await
        .context("Send welcome message")?;

    Ok((req_tx, ev_rx))
}

fn init_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    Ok(terminal)
}

fn restore_terminal() -> Result<()> {
    stdout().execute(LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}

fn install_panic_hook() {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        stdout().execute(LeaveAlternateScreen).unwrap();
        disable_raw_mode().unwrap();
        original_hook(panic_info);
    }));
}

