use std::io;
use std::time::Duration;
use std::process::Stdio;
use tokio::sync::{mpsc, oneshot};
use tokio::process::{Command, ChildStdin, ChildStdout};
use tokio::io::{BufWriter, AsyncWriteExt as _, BufReader, AsyncBufReadExt as _, Lines};
use tracing::{trace, debug, warn, error};
use shakmaty::fen::Fen;
use shakmaty::variants::VariantPosition;
use crate::api::Score;
use crate::ipc::{Position, PositionResponse, PositionFailed};
use crate::util::NevermindExt as _;

pub fn channel() -> (StockfishStub, StockfishActor) {
    let (tx, rx) = mpsc::channel(1);
    (StockfishStub { tx }, StockfishActor { rx })
}

pub struct StockfishStub {
    tx: mpsc::Sender<StockfishMessage>,
}

impl StockfishStub {
    pub async fn go(&mut self, position: Position) -> Result<PositionResponse, PositionFailed> {
        let (callback, response) = oneshot::channel();
        let batch_id = position.batch_id;
        self.tx.send(StockfishMessage::Go { position, callback }).await.map_err(|_| PositionFailed {
            batch_id,
        })?;
        response.await.map_err(|_| PositionFailed {
            batch_id,
        })
    }
}

pub struct StockfishActor {
    rx: mpsc::Receiver<StockfishMessage>,
}

#[derive(Debug)]
enum StockfishMessage {
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

#[derive(Debug)]
enum EngineError {
    IoError(io::Error),
    Shutdown,
}

impl From<io::Error> for EngineError {
    fn from(error: io::Error) -> EngineError {
        EngineError::IoError(error)
    }
}

#[cfg(unix)]
fn new_process_group(command: &mut Command) -> &mut Command {
    // Stop SIGINT from propagating to child process.
    unsafe {
        // Safety: The closure is run in a fork, and is not allowed to break
        // invariants by using raw handles.
        command.pre_exec(|| {
            libc::setpgid(0, 0);
            Ok(())
        })
    }
}

#[cfg(windows)]
fn new_process_group(command: &mut Command) -> &mut Command {
    // Stop CTRL+C from propagating to child process:
    // https://docs.microsoft.com/en-us/windows/win32/procthread/process-creation-flags
    let create_new_process_group = 0x00000200;
    command.creation_flags(create_new_process_group)
}

impl StockfishActor {
    pub async fn run(self) {
        if let Err(EngineError::IoError(err)) = self.run_inner().await {
            error!("Engine error: {}", err);
        }
    }

    async fn run_inner(mut self) -> Result<(), EngineError> {
        let mut child = new_process_group(
            Command::new("stockfish")
                .stdout(Stdio::piped())
                .stdin(Stdio::piped())
                .kill_on_drop(true)).spawn()?;

        let pid = child.id().expect("pid");
        let mut stdout = Stdout::new(pid, child.stdout.take().ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "stdout closed"))?);
        let mut stdin = Stdin::new(pid, child.stdin.take().ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "stdin closed"))?);

        Ok(loop {
            tokio::select! {
                msg = self.rx.recv() => {
                    if let Some(msg) = msg {
                        self.handle_message(&mut stdout, &mut stdin, msg).await?;
                    } else {
                        break;
                    }
                }
                status = child.wait() => {
                    match status? {
                        status if status.success() => debug!("Stockfish process {} exited with status {}", pid, status),
                        status => error!("Stockfish process {} exited with status {}", pid, status),
                    }
                    break;
                }
            }
        })
    }

    async fn handle_message(&mut self, stdout: &mut Stdout, stdin: &mut Stdin, msg: StockfishMessage) -> Result<(), EngineError> {
        Ok(match msg {
            StockfishMessage::Go { mut callback, position } => {
                tokio::select! {
                    _ = callback.closed() => return Err(EngineError::Shutdown),
                    res = self.go(stdout, stdin, position) => callback.send(res?).nevermind("go receiver dropped"),
                }
            }
        })
    }

    async fn go(&mut self, stdout: &mut Stdout, stdin: &mut Stdin, position: Position) -> io::Result<PositionResponse> {
        stdin.write_line("ucinewgame").await?;

        let fen = if let Some(fen) = position.fen {
            fen
        } else {
            Fen::from_setup(&VariantPosition::new(position.variant))
        };

        let moves = position.moves.iter().map(|m| m.to_string()).collect::<Vec<_>>().join(" ");
        stdin.write_line(&format!("position fen {} moves {}", fen, moves)).await?;

        stdin.write_line(&format!("go nodes {}", position.nodes)).await?;

        let mut score = None;
        let mut depth = None;
        let mut pv = Vec::new();
        let mut time = Duration::default();
        let mut nodes = 0;
        let mut nps = None;

        loop {
            let line = stdout.read_line().await?;
            let mut parts = line.split(" ");
            match parts.next() {
                Some("bestmove") => {
                    return Ok(PositionResponse {
                        batch_id: position.batch_id,
                        position_id: position.position_id,
                        url: position.url,
                        best_move: parts.next().and_then(|m| m.parse().ok()),
                        score: score.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing score"))?,
                        depth: depth.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing depth"))?,
                        pv,
                        time,
                        nodes,
                        nps,
                    });
                }
                Some("info") => {
                    while let Some(part) = parts.next() {
                        match part {
                            "depth" => {
                                depth = Some(
                                    parts.next()
                                        .and_then(|t| t.parse().ok())
                                        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "expected depth"))?);
                            }
                            "nodes" => {
                                nodes = parts.next()
                                    .and_then(|t| t.parse().ok())
                                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "expected nodes"))?;
                            }
                            "time" => {
                                time = parts.next()
                                    .and_then(|t| t.parse().ok())
                                    .map(Duration::from_millis)
                                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "expected time"))?;
                            }
                            "nps" => {
                                nps = parts.next().and_then(|n| n.parse().ok());
                            }
                            "score" => {
                                score = match parts.next() {
                                    Some("cp") => parts.next().and_then(|cp| cp.parse().ok()).map(Score::Cp),
                                    Some("mate") => parts.next().and_then(|mate| mate.parse().ok()).map(Score::Mate),
                                    _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "expected cp or mate")),
                                }
                            }
                            "pv" => {
                                pv.clear();
                                while let Some(part) = parts.next() {
                                    pv.push(part.parse().map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid pv"))?);
                                }
                            }
                            _ => (),
                        }
                    }
                }
                _ => warn!("Unexpected engine output: {}", line),
            }
        }
    }
}
