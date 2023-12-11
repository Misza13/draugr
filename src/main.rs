use anyhow::Context;
use clap::{Parser, arg};

use crate::telnet::*;
use crate::tui::*;

mod input;
mod panes;
mod ring;
mod telnet;
mod tui;

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    address: Option<String>,

    #[arg(short, long, default_value_t = 4000)]
    port: u16,
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

    tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(event) = telnet_rx.recv() => {
                    match event {
                        TelnetEvent::Data(data) => {
                            tui_tx.send(TuiRequest::Print(data, 1)).await?;
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
                            break;
                        },
                    }
                },
            }
        }

        Ok::<(), anyhow::Error>(())
    }).await??;

    Ok(())
}
