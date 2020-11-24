use std::io;
use std::time::Duration;
use std::process::Stdio;
use tokio::sync::{mpsc, oneshot};
use tokio::process::{Command, ChildStdin, ChildStdout};
use tokio::io::{BufWriter, AsyncWriteExt as _, BufReader, AsyncBufReadExt as _, Lines};
use tracing::{trace, debug, info, warn, error};
use shakmaty::fen::Fen;
use shakmaty::variants::VariantPosition;
use crate::ipc::{Position, PositionResponse};
use crate::util::{NevermindExt as _};

pub fn channel() -> (StockfishStub, StockfishActor) {
    let (tx, rx) = mpsc::channel(1);
    (StockfishStub { tx }, StockfishActor { rx })
}

pub struct StockfishStub {
    tx: mpsc::Sender<StockfishMessage>,
}

impl StockfishStub {
    pub async fn ping(&mut self) -> Result<(), StockfishError> {
        let (pong, ping) = oneshot::channel();
        self.tx.send(StockfishMessage::Ping { pong }).await.map_err(|_| StockfishError)?;
        ping.await.map_err(|_| StockfishError)
    }

    pub async fn go(&mut self, position: Position) -> Result<PositionResponse, StockfishError> {
        let (callback, response) = oneshot::channel();
        self.tx.send(StockfishMessage::Go { position, callback }).await.map_err(|_| StockfishError)?;
        response.await.map_err(|_| StockfishError)
    }
}

#[derive(Debug)]
pub struct StockfishError;

pub struct StockfishActor {
    rx: mpsc::Receiver<StockfishMessage>,
}

#[derive(Debug)]
enum StockfishMessage {
    Ping {
        pong: oneshot::Sender<()>,
    },
    Go {
        position: Position,
        callback: oneshot::Sender<PositionResponse>,
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
                    _ = pong.closed() => return Err(io::Error::new(io::ErrorKind::Other, "pong receiver dropped")),
                    res = self.ping(stdout, stdin) => pong.send(res?).nevermind("pong receiver dropped"),
                }
            },
            StockfishMessage::Go { mut callback, position } => {
                tokio::select! {
                    _ = callback.closed() => return Err(io::Error::new(io::ErrorKind::Other, "go receiver dropped")),
                    res = self.go(stdout, stdin, position) => callback.send(res?).nevermind("go receiver dropped"),
                }
            }
        })
    }

    async fn ping(&mut self, stdout: &mut Stdout, stdin: &mut Stdin) -> io::Result<()> {
        stdin.write_line("quit").await?;
        loop {
            let line = stdout.read_line().await?;
            if line == "readyok" {
                return Ok(());
            } else {
                warn!("Unexpected engine output: {}", line);
            }
        }
    }

    async fn go(&mut self, stdout: &mut Stdout, stdin: &mut Stdin, position: Position) -> io::Result<PositionResponse> {
        let fen = if let Some(fen) = position.fen {
            fen
        } else {
            Fen::from_setup(&VariantPosition::new(position.variant))
        };
        let moves = position.moves.iter().map(|m| m.to_string()).collect::<Vec<_>>().join(" ");
        stdin.write_line(&format!("position fen {} moves {}", fen, moves)).await?;

        let go = format!("go nodes {}", position.nodes);
        stdin.write_line(&go).await?;

        let mut score = None;
        let mut pv = Vec::new();
        let mut depth = None;
        let mut nodes = None;
        let mut time = None;
        let mut nps = None;

        loop {
            let line = stdout.read_line().await?;
            let mut parts = line.split(" ");
            let command = parts.next().expect("non-empty split");
            if command == "bestmove" {
                let best_move = parts.next().and_then(|m| m.parse().ok());
                return Ok(PositionResponse {
                    batch_id: position.batch_id,
                    position_id: position.position_id,
                    score: score.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing score"))?,
                    best_move,
                    pv,
                    depth: depth.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing depth"))?,
                    nodes: nodes.unwrap_or(0),
                    time: time.unwrap_or(Duration::default()),
                    nps,
                });
            }
        }
    }
}
