use anyhow::{Context, Result, anyhow};
use telnet::{Event, TelnetOption};
use tokio::sync::mpsc::{channel, Sender, Receiver};

pub enum TelnetRequest {
    Connect(String, u16),
    Send(String),
    #[allow(dead_code)] // TODO
    Disconnect,
    Shutdown,
}

pub enum TelnetEvent {
    Data(String),
    Unhandled(Event),
    Error(anyhow::Error),
}

pub fn telnet_connection() -> Result<(Sender<TelnetRequest>, Receiver<TelnetEvent>)> {
    let (req_tx, req_rx) = channel(1024);
    let (ev_tx, ev_rx) = channel(1024);

    tokio::task::spawn_blocking(move || {
        let mut telnet = TelnetConnection {
            telnet: None,
            rx: req_rx,
            tx: ev_tx,
        };

        loop {
            // Handle receiving from socket
            telnet.handle_telnet_recv()
                .context("Handle telnet recv")?;

            // Handle requests and sending to socket
            if telnet.handle_request()
                .context("Handle request")? {
                    break;
                }
        }

        Ok::<(), anyhow::Error>(())
    });

    Ok((req_tx, ev_rx))
}

struct TelnetConnection {
    telnet: Option<telnet::Telnet>,
    rx: Receiver<TelnetRequest>,
    tx: Sender<TelnetEvent>,
}

impl TelnetConnection {
    fn connect(&mut self, address: String, port: u16) -> Result<()> {
        self.telnet = Some(
            telnet::Telnet::connect((address, port), 1024*1024)
                .context("Connect to server")?);

        Ok(())
    }

    fn reset_connection(&mut self) {
        self.telnet = None;
    }

    fn notify_of_error(&mut self, err: anyhow::Error) -> Result<()> {
        self.tx.blocking_send(TelnetEvent::Error(err))
            .context("Notify of error")
    }

    fn handle_telnet_recv(&mut self) -> Result<()> {
        if let Err(err) = self.handle_telnet_recv_impl() {
            // Assume socket is bad
            self.reset_connection();
            self.notify_of_error(err)?;
        }

        Ok(())
    }

    fn handle_telnet_recv_impl(&mut self) -> Result<()> {
        if let Some(telnet) = &mut self.telnet {
            let event = telnet.read_timeout(std::time::Duration::from_millis(20))
                .context("Read from socket")?;

            match event {
                Event::TimedOut => {},
                Event::Data(data) => {
                    let s = String::from_utf8(data.into())
                        .context("Decode data to UTF-8 string")?;
                    self.tx.blocking_send(TelnetEvent::Data(s))
                        .context("Send data over channel")?;
                },
                Event::Negotiation(telnet::Action::Will, TelnetOption::Compress2) => {
                    telnet.negotiate(&telnet::Action::Do, TelnetOption::Compress2)
                        .context("Negotiate MCCP2")?;
                },
                Event::Negotiation(_, _) => {},
                Event::Subnegotiation(TelnetOption::Compress2, _) => {
                    telnet.begin_zlib();
                },
                Event::Subnegotiation(_, _) => {},
                _ => {
                    self.tx.blocking_send(TelnetEvent::Unhandled(event))
                        .context("Notify of unhandled telnet event")?;
                },
            }
        };

        Ok(())
    }

    fn handle_request(&mut self) -> Result<bool> {
        match self.handle_request_impl() {
            Ok(shutdown) => { Ok(shutdown) },
            Err(err) => {
                self.notify_of_error(err)
                    .map(|_| false)
                    .context("Notify of error")
            },
        }
    }

    fn handle_request_impl(&mut self) -> Result<bool> {
        if let Ok(request) = self.rx.try_recv() {
            match request {
                TelnetRequest::Connect(address, port) => {
                    self.connect(address, port)
                        .context("Connect to server")?;
                },
                TelnetRequest::Send(data) => {
                    if let Some(telnet) = &mut self.telnet {
                        telnet.write(data.as_bytes())
                            .context("Write data to socket")?;
                        telnet.write(b"\n")
                            .context("Write newline to socket")?;
                    } else {
                        return Err(anyhow!("Connection is closed"));
                    }
                },
                TelnetRequest::Disconnect => {
                    if self.telnet.is_some() {
                        return Ok(true);
                    } else {
                        return Err(anyhow!("Connection is closed"));
                    }
                },
                TelnetRequest::Shutdown => {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }
}
