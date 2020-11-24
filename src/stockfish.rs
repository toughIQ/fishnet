use std::io;
use std::process::Stdio;
use tokio::sync::{mpsc, oneshot};
use tokio::process::{Command, ChildStdin};
use tokio::io::{BufWriter, AsyncWriteExt as _, BufReader, AsyncBufReadExt as _};
use tracing::{trace, info, error};
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
        Ok({
            trace!("{} << {}", self.pid, line);
            self.inner.write_all(line.as_bytes()).await?;
            self.inner.write_all(b"\n").await?;
            self.inner.flush().await?;
        })
    }
}

impl StockfishActor {
    pub async fn run(mut self) {
        let mut child = Command::new("stockfish")
            .stdout(Stdio::piped())
            .stdin(Stdio::piped())
            .spawn().expect("failed to spawn stockfish");

        let pid = child.id().expect("pid");
        let mut stdout = BufReader::new(child.stdout.take().expect("pipe stdout")).lines();
        let mut stdin = Stdin::new(pid, child.stdin.take().expect("pipe stdin"));

        let join_handle = tokio::spawn(async move {
            match child.wait().await {
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
        });

        while let Some(msg) = self.rx.recv().await {
            match msg {
                StockfishMessage::Ping { pong } => {
                    stdin.write_line("isready").await.expect("write isready"); // TODO
                    while let Ok(Some(line)) = stdout.next_line().await {
                        trace!("{} >> {} ", pid, line);
                        if line == "readyok" {
                            pong.send(()).nevermind("pong receiver dropped");
                            break;
                        }
                    }
                }
            }
        }

        join_handle.await.expect("join");
    }
}
