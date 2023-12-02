use anyhow::Context;
use telnet::{Event, TelnetOption};
use tokio::sync::mpsc::{channel, Sender, Receiver};

pub enum TelnetRequest {
    Send(String),
    Disconnect,
}

pub enum TelnetEvent {
    Data(String),
    Unhandled(Event),
    Error(anyhow::Error),
}

pub fn telnet_connection() -> Result<(Sender<TelnetRequest>, Receiver<TelnetEvent>), anyhow::Error> {
    let (req_tx, mut req_rx) = channel(1024);
    let (ev_tx, ev_rx) = channel(1024);

    tokio::task::spawn_blocking(move || {
        let mut connection = telnet::Telnet::connect(("aardmud.org", 4000), 1024*1024)
            .context("Connect to server")?;

        loop {
            let event = connection.read_timeout(std::time::Duration::from_millis(20))
                .context("Read from socket")?;

            match event {
                Event::TimedOut => {},
                Event::Data(data) => {
                    let s = String::from_utf8(data.into())
                        .context("Decode data to UTF-8 string")?;
                    ev_tx.blocking_send(TelnetEvent::Data(s))?;
                },
                Event::Negotiation(telnet::Action::Will, TelnetOption::Compress2) => {
                    connection.negotiate(&telnet::Action::Do, TelnetOption::Compress2)?;
                },
                Event::Negotiation(_, _) => {},
                Event::Subnegotiation(TelnetOption::Compress2, _) => {
                    connection.begin_zlib();
                },
                Event::Subnegotiation(_, _) => {},
                _ => {
                    ev_tx.blocking_send(TelnetEvent::Unhandled(event))?;
                },
            }

            if let Ok(request) = req_rx.try_recv() {
                match handle_request(request, &mut connection) {
                    Ok(true) => {
                        break;
                    },
                    Ok(false) => {},
                    Err(err) => {
                        ev_tx.blocking_send(TelnetEvent::Error(err))?;
                    }
                }
            }
        }

        Ok::<(), anyhow::Error>(())
    });

    Ok((req_tx, ev_rx))
}

fn handle_request(request: TelnetRequest, connection: &mut telnet::Telnet) -> Result<bool, anyhow::Error> {
    match request {
        TelnetRequest::Send(data) => {
            connection.write(data.as_bytes())
                .context("Write data to socket")?;
            connection.write(b"\n")
                .context("Write newline")?;
        }
        TelnetRequest::Disconnect => {
            return Ok(true);
        }
    }

    Ok(false)
}
