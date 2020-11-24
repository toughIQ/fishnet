use std::io;
use std::process::Stdio;
use tokio::sync::{mpsc, oneshot};
use tokio::process::{Command, ChildStdin, ChildStdout};
use tokio::io::{BufWriter, AsyncWriteExt as _, BufReader, AsyncBufReadExt as _, Lines};
use tracing::{trace, debug, info, warn, error};
use crate::util::{NevermindExt as _};

pub fn channel() -> (StockfishStub, StockfishActor) {
    let (tx, rx) = mpsc::channel(1);
    (StockfishStub { tx }, StockfishActor { rx })
}

pub struct StockfishStub {
    tx: mpsc::Sender<StockfishMessage>,
}

impl StockfishStub {
    pub async fn ping(&mut self) -> Option<()> {
        let (pong, ping) = oneshot::channel();
        self.tx.send(StockfishMessage::Ping { pong }).await.expect("stockfish actor alive");
        ping.await.ok()
    }
}

pub struct StockfishActor {
    rx: mpsc::Receiver<StockfishMessage>,
}

#[derive(Debug)]
enum StockfishMessage {
    Ping {
        pong: oneshot::Sender<()>,
    },
}

struct Stdin {
    pid: u32,
    inner: BufWriter<ChildStdin>,
}

impl Stdin {
    fn new(pid: u32, inner: ChildStdin) -> Stdin {
        Stdin {
            pid,
            inner: BufWriter::new(inner),
        }
    }

    async fn write_line(&mut self, line: &str) -> io::Result<()> {
        trace!("{} << {}", self.pid, line);
        self.inner.write_all(line.as_bytes()).await?;
        self.inner.write_all(b"\n").await?;
        self.inner.flush().await?;
        Ok(())
    }
}

struct Stdout {
    pid: u32,
    inner: Lines<BufReader<ChildStdout>>,
}

impl Stdout {
    fn new(pid: u32, inner: ChildStdout) -> Stdout {
        Stdout {
            pid,
            inner: BufReader::new(inner).lines(),
        }
    }

    async fn read_line(&mut self) -> io::Result<String> {
        if let Some(line) = self.inner.next_line().await? {
            trace!("{} >> {}", self.pid, line);
            Ok(line)
        } else {
            Err(io::ErrorKind::UnexpectedEof.into())
        }
    }
}

impl StockfishActor {
    pub async fn run(mut self) {
        let mut child = Command::new("stockfish")
            .stdout(Stdio::piped())
            .stdin(Stdio::piped())
            .spawn().expect("failed to spawn stockfish");

        let pid = child.id().expect("pid");
        let mut stdout = Stdout::new(pid, child.stdout.take().expect("pipe stdout"));
        let mut stdin = Stdin::new(pid, child.stdin.take().expect("pipe stdin"));

        loop {
            tokio::select! {
                msg = self.rx.recv() => {
                    if let Some(msg) = msg {
                        if let Err(err) = self.handle_message(&mut stdout, &mut stdin, msg).await {
                            error!("Engine error: {}", err);
                            break;
                        }
                    } else {
                        break;
                    }
                }
                status = child.wait() => {
                    match status {
                        Ok(status) if status.success() => {
                            info!("Stockfish process exited with status {}", status);
                        }
                        Ok(status) => {
                            error!("Stockfish process exited with status {}", status);
                        }
                        Err(err) => {
                            error!("Stockfish process dead: {}", err);
                        }
                    }
                    break;
                }
            }
        }

        debug!("Shutting down Stockfish process {}.", pid);
        child.kill().await.nevermind("kill");
    }

    async fn handle_message(&mut self, stdout: &mut Stdout, stdin: &mut Stdin, msg: StockfishMessage) -> io::Result<()> {
        Ok(match msg {
            StockfishMessage::Ping { mut pong } => {
                tokio::select! {
                    _ = pong.closed() => return Ok(()),
                    res = stdin.write_line("quit") => res?,
                }

                loop {
                    let line = tokio::select! {
                        _ = pong.closed() => return Ok(()),
                        line = stdout.read_line() => line?,
                    };

                    if line == "readyok" {
                        pong.send(()).nevermind("pong receiver dropped");
                        break;
                    } else {
                        warn!("Unexpected engine output: {}", line);
                    }
                }
            }
        })
    }
}
