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
    Info(String),
    Warning(String),
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
        self.send_info(format!("Connecting to {address}:{port}..."))
            .context("Inform about connection attempt")?;

        self.telnet = Some(
            telnet::Telnet::connect((address, port), 1024*1024)
                .context("Connect to server")?);

        self.send_info("Connected.".into())
            .context("Inform about successful connection")?;

        Ok(())
    }

    fn reset_connection(&mut self) -> Result<()> {
        self.telnet = None;

        self.send_warning("Disconnected.".into())
            .context("Warn about broken connection")?;

        Ok(())
    }

    fn send_info(&mut self, data: String) -> Result<()> {
        self.tx.blocking_send(TelnetEvent::Info(data))
            .context("Send info from telnet")
    }

    fn send_warning(&mut self, data: String) -> Result<()> {
        self.tx.blocking_send(TelnetEvent::Warning(data))
            .context("Send warning from telnet")
    }

    fn send_error(&mut self, err: anyhow::Error) -> Result<()> {
        self.tx.blocking_send(TelnetEvent::Error(err))
            .context("Send error from telnet")
    }

    fn handle_telnet_recv(&mut self) -> Result<()> {
        if let Err(err) = self.handle_telnet_recv_impl() {
            // Assume socket is bad
            self.send_error(err)
                .context("Send error information")?;

            self.reset_connection()
                .context("Reset connection")?;
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
                Event::UnknownIAC(249) => { /* IAC GO AHEAD - used as end-of-prompt signal in some MUDs */},
                Event::Negotiation(telnet::Action::Will, TelnetOption::Compress2) => {
                    self.tx.blocking_send(TelnetEvent::Info("Server supports MCCP2".into()))
                        .context("Inform of MCCP2 capability")?;

                    telnet.negotiate(&telnet::Action::Do, TelnetOption::Compress2)
                        .context("Negotiate MCCP2")?;
                },
                Event::Negotiation(_, _) => {},
                Event::Subnegotiation(TelnetOption::Compress2, _) => {
                    telnet.begin_zlib();

                    self.tx.blocking_send(TelnetEvent::Info("MCCP2 enabled".into()))
                        .context("Inform of MCCP2 enabled")?;
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
                self.send_error(err)
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
