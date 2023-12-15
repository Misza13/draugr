use anyhow::{Context, Result};
use clap::{Parser, arg};
use tokio::sync::mpsc::Sender;

use crate::script::*;
use crate::telnet::*;
use crate::tui::*;

mod ring;
mod script;
mod telnet;
mod tui;

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    address: Option<String>,

    #[arg(short, long, default_value_t = 4000)]
    port: u16,

    #[arg(short, long)]
    script: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let (tui_tx, mut tui_rx) = create_tui().await
        .context("Create TUI")?;

    let (telnet_tx, mut telnet_rx) = telnet_connection()
        .context("Create connection")?;

    if let Some(address) = args.address {
        telnet_tx.send(TelnetRequest::Connect(address, args.port)).await
            .context("Connect from command line")?;
    }

    let (script_tx, mut script_rx) = create_script_engine()
        .context("Create script engine")?;

    if let Some(script) = args.script {
        script_tx.send(ScriptEngineRequest::ExecuteScriptFile(script)).await
            .context("Execute startup script")?;
    }

    let app = App { telnet_tx, tui_tx, script_tx };

    tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(event) = telnet_rx.recv() =>
                    app.handle_telnet_event(event).await
                        .context("Handle Telnet event")?,

                Some(event) = tui_rx.recv() => {
                    if app.handle_tui_event(event).await
                        .context("Handle TUI event")? { break; }
                },

                Some(event) = script_rx.recv() =>
                    app.handle_script_event(event).await
                        .context("Handle script event")?,
            }
        }

        Ok::<(), anyhow::Error>(())
    }).await??;

    Ok(())
}

struct App {
    telnet_tx: Sender<TelnetRequest>,
    tui_tx: Sender<TuiRequest>,
    script_tx: Sender<ScriptEngineRequest>,
}

impl App {
    async fn handle_telnet_event(&self, event: TelnetEvent) -> Result<()> {
        match event {
            TelnetEvent::Data(data) => {
                self.tui_tx.send(TuiRequest::Print(data.clone(), 1)).await
                    .context("Send output to TUI")?;

                self.script_tx.send(ScriptEngineRequest::Output(data)).await
                    .context("Send output to script engine")?;
            },
            TelnetEvent::Unhandled(event) => {
                self.tui_tx.send(TuiRequest::PrintWarning(format!("Unhandled telnet event: {:?}", event), 1)).await
                    .context("Send warning about unhandled event to TUI")?;
            },
            TelnetEvent::Info(data) => {
                self.tui_tx.send(TuiRequest::PrintInfo(data, 1)).await
                    .context("Send INFO to TUI")?;
            },
            TelnetEvent::Warning(data) => {
                self.tui_tx.send(TuiRequest::PrintWarning(data, 1)).await
                    .context("Send WARN to TUI")?;
            },
            TelnetEvent::Error(err) => {
                self.tui_tx.send(TuiRequest::PrintError(format!("{:?}", err.context("Connection error")), 1)).await
                    .context("Send ERR to TUI")?;
            },
        }

        Ok(())
    }

    async fn handle_tui_event(&self, event: TuiEvent) -> Result<bool> {
        match event {
            TuiEvent::Send(data) => {
                self.telnet_tx.send(TelnetRequest::Send(data.clone())).await
                    .context("Send data to Telnet")?;
            },
            TuiEvent::SendSecret(data) => {
                self.telnet_tx.send(TelnetRequest::Send(data.clone())).await
                    .context("Send data to Telnet")?;

                self.tui_tx.send(TuiRequest::PrintUserInput("*****".into(), 1)).await
                    .context("Echo user input (masked)")?;
            },
            TuiEvent::Quit => {
                self.telnet_tx.send(TelnetRequest::Shutdown).await
                    .context("Send shutdown signal to Telnet")?;

                self.script_tx.send(ScriptEngineRequest::Shutdown).await
                    .context("Send shutdown signal to script engine")?;

                return Ok(true);
            },
        }

        Ok(false)
    }

    async fn handle_script_event(&self, event: ScriptEngineEvent) -> Result<()> {
        match event {
            ScriptEngineEvent::Connect(address, port) => {
                self.telnet_tx.send(TelnetRequest::Connect(address, port)).await
                    .context("Send connect request to Telnet")?;
            },
            ScriptEngineEvent::Send(data) => {
                self.telnet_tx.send(TelnetRequest::Send(data.clone())).await
                    .context("Send data to Telnet")?;

                self.tui_tx.send(TuiRequest::PrintUserInput(data, 1)).await
                    .context("Echo user input")?;
            },
            ScriptEngineEvent::SendSecret(data) => {
                self.telnet_tx.send(TelnetRequest::Send(data.clone())).await
                    .context("Send data to Telnet")?;

                self.tui_tx.send(TuiRequest::PrintUserInput("*****".into(), 1)).await
                    .context("Echo user input (masked)")?;
            },
            ScriptEngineEvent::SetLayout(layout) => {
                self.tui_tx.send(TuiRequest::SetLayout(layout)).await
                    .context("Set layout")?;
            },
            ScriptEngineEvent::Error(err) => {
                self.tui_tx.send(TuiRequest::PrintError(format!("{:?}", err.context("Script error")), 1)).await
                    .context("Display script error")?;
            },
        }

        Ok(())
    }
}