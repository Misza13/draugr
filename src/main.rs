use anyhow::Context;
use clap::{Parser, arg};

use crate::script::*;
use crate::telnet::*;
use crate::tui::*;

mod input;
mod panes;
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
async fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();

    let (tui_tx, mut tui_rx) = create_tui()
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

    tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(event) = telnet_rx.recv() => {
                    match event {
                        TelnetEvent::Data(data) => {
                            tui_tx.send(TuiRequest::Print(data.clone(), 1)).await?;
                            script_tx.send(ScriptEngineRequest::Output(data)).await?;
                        },
                        TelnetEvent::Unhandled(event) => {
                            tui_tx.send(TuiRequest::PrintWarning(format!("Unhandled telnet event: {:?}", event), 1)).await?;
                        },
                        TelnetEvent::Info(data) => {
                            tui_tx.send(TuiRequest::PrintInfo(data, 1)).await?;
                        },
                        TelnetEvent::Warning(data) => {
                            tui_tx.send(TuiRequest::PrintWarning(data, 1)).await?;
                        },
                        TelnetEvent::Error(err) => {
                            tui_tx.send(TuiRequest::PrintError(format!("{:?}", err.context("Connection error")), 1)).await?;
                        },
                    }
                },

                Some(event) = tui_rx.recv() => {
                    match event {
                        TuiEvent::Send(data) => {
                            telnet_tx.send(TelnetRequest::Send(data.clone())).await?;
                            tui_tx.send(TuiRequest::PrintUserInput(data, 1)).await?;
                        },
                        TuiEvent::SendSecret(data) => {
                            telnet_tx.send(TelnetRequest::Send(data.clone())).await?;
                            tui_tx.send(TuiRequest::PrintUserInput("*****".into(), 1)).await?;
                        },
                        TuiEvent::Quit => {
                            telnet_tx.send(TelnetRequest::Shutdown).await?;
                            script_tx.send(ScriptEngineRequest::Shutdown).await?;
                            break;
                        },
                    }
                },

                Some(event) = script_rx.recv() => {
                    match event {
                        ScriptEngineEvent::Connect(address, port) => {
                            telnet_tx.send(TelnetRequest::Connect(address, port)).await?;
                        },
                        ScriptEngineEvent::Send(data) => {
                            telnet_tx.send(TelnetRequest::Send(data.clone())).await?;
                            tui_tx.send(TuiRequest::PrintUserInput(data, 1)).await?;
                        },
                        ScriptEngineEvent::SendSecret(data) => {
                            telnet_tx.send(TelnetRequest::Send(data.clone())).await?;
                            tui_tx.send(TuiRequest::PrintUserInput("*****".into(), 1)).await?;
                        },
                        ScriptEngineEvent::Error(err) => {
                            tui_tx.send(TuiRequest::PrintError(format!("{:?}", err.context("Script error")), 1)).await?;
                        },
                    }
                },
            }
        }

        Ok::<(), anyhow::Error>(())
    }).await??;

    Ok(())
}
