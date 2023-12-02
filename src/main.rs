use anyhow::Context;
use crate::telnet::*;
use crate::tui::*;

mod telnet;
mod tui;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let (tui_tx, mut tui_rx) = create_tui()
        .context("Create TUI")?;

    let (telnet_tx, mut telnet_rx) = telnet_connection()
        .context("Create connection")?;

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
                        TelnetEvent::Error(err) => {
                            tui_tx.send(TuiRequest::PrintError(format!("{:?}", err), 1)).await?;
                        },
                    }
                },

                Some(event) = tui_rx.recv() => {
                    match event {
                        TuiEvent::Send(data) => {
                            telnet_tx.send(TelnetRequest::Send(data.clone())).await?;
                            tui_tx.send(TuiRequest::PrintUserInput(data, 1)).await?;
                        },
                        TuiEvent::Quit => {
                            telnet_tx.send(TelnetRequest::Disconnect).await?;
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
