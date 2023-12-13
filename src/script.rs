use regex::Regex;
use tokio::sync::mpsc::{channel, Sender, Receiver};
use tokio::sync::oneshot;
use anyhow::{Context, Result, Error};
use rhai::{Engine, EvalAltResult};

pub enum ScriptEngineRequest {
    Output(String),
    ExecuteScriptFile(String),
    Shutdown,
}

pub enum ScriptEngineEvent {
    Connect(String, u16),
    Send(String),
    SendSecret(String),
    Error(anyhow::Error),
}

enum ScriptEvent {
    Expect(String, oneshot::Sender<String>),
}

struct ScriptEngine {
    ev_tx: Sender<ScriptEngineEvent>,
    i_tx: Sender<ScriptEvent>,

    expects: Vec<(Regex, oneshot::Sender<String>)>,
}

type ScriptFunctionResult<T> = Result<T, Box<EvalAltResult>>;

pub fn create_script_engine() -> Result<(Sender<ScriptEngineRequest>, Receiver<ScriptEngineEvent>)> {
    let (req_tx, mut req_rx) = channel(256);
    let (ev_tx, ev_rx) = channel(256);
    let (i_tx, mut i_rx) = channel(256);

    tokio::spawn(async move {
        let mut engine = ScriptEngine {
            expects: vec![],
            ev_tx,
            i_tx,
        };

        loop {
            tokio::select! {
                Some(request) = req_rx.recv() => {
                    match engine.handle_request(request).await {
                        Ok(true) => { break; },
                        Ok(false) => {},
                        Err(err) => {
                            engine.ev_tx.send(ScriptEngineEvent::Error(err)).await
                                .context("Notify of script request handler error")?;
                        },
                    }
                },

                Some(event) = i_rx.recv() => {
                    if let Err(err) = engine.handle_script_event(event) {
                        engine.ev_tx.send(ScriptEngineEvent::Error(err)).await
                            .context("Notify of script error")?;
                    }
                },
            }
        }

        Ok::<(), anyhow::Error>(())
    });

    Ok((req_tx, ev_rx))
}

impl ScriptEngine {
    async fn handle_request(&mut self, request: ScriptEngineRequest) -> Result<bool> {
        match request {
            ScriptEngineRequest::Output(data) => {
                let matches: Vec<_> = self.expects.iter()
                    .enumerate()
                    .filter(|(_, (pattern, _))| pattern.is_match(&data))
                    .map(|(idx, _)| idx)
                    .collect();

                for idx in matches {
                    let (_, tx) = self.expects.remove(idx);

                    tx.send(data.clone())
                        .map_err(|err| anyhow::format_err!("{err}"))
                        .context("Send expect data back to script")?;
                }
            },
            ScriptEngineRequest::ExecuteScriptFile(path) => {
                let script = std::fs::read_to_string(path)
                    .context("Read script file")?;

                self.execute_script(script)
                    .context("Execute script")?;
            },
            ScriptEngineRequest::Shutdown => { return Ok(true) },
        }

        Ok(false)
    }

    fn handle_script_event(&mut self, event: ScriptEvent) -> Result<()> {
        match event {
            ScriptEvent::Expect(pattern, tx) => {
                let pattern = Regex::new(&pattern)
                    .context("Compile pattern expression")?;
                self.expects.push((pattern, tx));
            }
        }

        Ok(())
    }

    fn execute_script(&mut self, script: String) -> Result<()> {
        let ev_tx = self.ev_tx.clone();
        let i_tx = self.i_tx.clone();

        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut engine = Engine::new();

            let ev_tx_cl = ev_tx.clone();
            engine.register_fn("connect", move |address: String, port: i64| -> ScriptFunctionResult<()> {
                ev_tx_cl.blocking_send(ScriptEngineEvent::Connect(address, port as u16))
                    .context("Emit connection request")
                    .map_err(script_error_mapper)
            });

            let i_tx_cl = i_tx.clone();
            engine.register_fn("expect", move |expect: String| -> ScriptFunctionResult<String> {
                let (tx, rx) = oneshot::channel();

                i_tx_cl.blocking_send(ScriptEvent::Expect(expect, tx))
                    .context("Emit expect event")
                    .map_err(script_error_mapper)?;

                rx.blocking_recv()
                    .context("Wait for expectation to be satisfied")
                    .map_err(script_error_mapper)
            });

            let ev_tx_cl = ev_tx.clone();
            engine.register_fn("send", move |text: String| -> ScriptFunctionResult<()> {
                ev_tx_cl.blocking_send(ScriptEngineEvent::Send(text))
                    .context("Emit send event")
                    .map_err(script_error_mapper)
            });

            let ev_tx_cl = ev_tx.clone();
            engine.register_fn("send_secret", move |text: String| -> ScriptFunctionResult<()> {
                ev_tx_cl.blocking_send(ScriptEngineEvent::SendSecret(text))
                    .context("Emit send secret event")
                    .map_err(script_error_mapper)
            });

            if let Err(err) = engine.run(&script) {
                ev_tx.blocking_send(ScriptEngineEvent::Error(
                    anyhow::format_err!("{err}").context("Run script engine")))?;
            }

            Ok(())
        });

        Ok(())
    }
}

fn script_error_mapper(err: Error) -> Box<EvalAltResult> {
    format!("{:?}", err).into()
}