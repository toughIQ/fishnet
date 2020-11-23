use url::Url;
use std::cmp::min;
use std::time::Duration;
use reqwest::StatusCode;
use tokio::time;
use tokio::sync::{mpsc, oneshot};
use tracing::{warn, error};
use rand::Rng;
use crate::configure::{Key, KeyError};
use crate::util::WhateverExt as _;

pub fn spawn(endpoint: Url) -> ApiStub {
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        ApiActor::new(rx, endpoint).run().await;
    });
    ApiStub { tx }
}

#[derive(Debug)]
enum ApiMessage {
    CheckKey {
        key: Key,
        callback: oneshot::Sender<Result<Key, KeyError>>,
    }
}

/* struct Acquire {
    fishnet: Fishnet,
    stockfish: Stockfish,
}

struct Fishnet {
    version: &'static str,
    python: &'static str,
    apikey: String,
}

impl From<Key> for Fishnet {
    fn from(Key(apikey): Key) -> Fishnet {
        Fishnet {
            version: env!("CARGO_PKG_VERSION"),
            python: "-",
            apikey,
        }
    }
}

struct Stockfish {
    name: &'static str,
    options: StockfishOptions,
}

impl Default for Stockfish {
    fn default() -> Stockfish {
        Stockfish {
            name: "Stockfish 12+",
            options: StockfishOptions::default(),
        }
    }
}

struct StockfishOptions {
    hash: u32,
    threads: usize,
}

impl Default for StockfishOptions {
    fn default() -> StockfishOptions {
        StockfishOptions {
            hash: 32,
            threads: 1,
        }
    }
}

struct Work {
    tpe: WorkType, // type
    id: String,
}

enum WorkType {
    Analysis,
    Move,
}

enum Acquired {
    Ok {
        work: Work,
        game_id: Option<String>,
        position: Option<String>,
        variant: Option<String>,
        moves: Option<String>,
        nodes: Option<u64>,
        skip_positions: Vec<usize>,
    },
    NoContent,
}

struct Analysis {
    fishnet: Fishnet,
    stockfish: Stockfish,
    analysis: Vec<AnalysisPart>,
}

enum AnalysisPart {
    Complete {
        pv: Option<String>,
        depth: u64,
        score: Score,
        time: Option<u64>,
        nodes: Option<u64>,
        nps: Option<u64>,
    },
    Skipped {
        skipped: bool,
    }
}

enum Score {
    Cp(i64),
    Mate(i64),
}

struct Move {
    fishnet: Fishnet,
    stockfish: Stockfish,
    bestmove: Option<String>,
}

struct Query {
    slow: bool,
    stop: bool,
}

struct Abort {
    fishnet: Fishnet,
    stockfish: Stockfish,
}

struct Status {
    analysis: AnalysisStatus,
}

struct AnalysisStatus {
    user: QueueStatus,
    system: QueueStatus,
}

struct QueueStatus {
    acquired: u64,
    queued: u64,
    oldest: u64,
} */

#[derive(Debug, Clone)]
pub struct ApiStub {
    tx: mpsc::UnboundedSender<ApiMessage>,
}

impl ApiStub {
    pub async fn check_key(&mut self, key: Key) -> Option<Result<Key, KeyError>> {
        let (req, res) = oneshot::channel();
        self.tx.send(ApiMessage::CheckKey {
            key,
            callback: req,
        }).expect("api actor alive");
        res.await.ok()
    }
}

struct ApiActor {
    rx: mpsc::UnboundedReceiver<ApiMessage>,
    endpoint: Url,
    client: reqwest::Client,
    error_backoff: RandomizedBackoff,
}

impl ApiActor {
    fn new(rx: mpsc::UnboundedReceiver<ApiMessage>, endpoint: Url) -> ApiActor {
        ApiActor {
            rx,
            endpoint,
            client: reqwest::Client::builder()
                .user_agent(concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")))
                .timeout(Duration::from_secs(15))
                .build().expect("client"),
            error_backoff: RandomizedBackoff::default(),
        }
    }

    async fn run(mut self) {
        while let Some(msg) = self.rx.recv().await {
            if let Err(err) = self.handle_message(msg).await {
                match err.status() {
                    Some(status) if status == StatusCode::TOO_MANY_REQUESTS => {
                        let backoff = Duration::from_secs(60) + self.error_backoff.next();
                        error!("Too many requests. Suspending requests for {:?}.", backoff);
                        time::delay_for(backoff).await;
                    }
                    Some(status) if status.is_server_error() => {
                        let backoff = self.error_backoff.next();
                        error!("Server error: {}. Backing off {:?}.", status, backoff);
                        time::delay_for(backoff).await;
                    }
                    Some(_) => self.error_backoff.reset(),
                    None => {
                        let backoff = self.error_backoff.next();
                        error!("Network error: {}. Backing off {:?}.", err, backoff);
                        time::delay_for(backoff).await;
                    }
                }
            } else {
                self.error_backoff.reset();
            }
        }
    }

    async fn handle_message(&mut self, msg: ApiMessage) -> reqwest::Result<()> {
        Ok(match msg {
            ApiMessage::CheckKey { key, callback } => {
                let url = format!("{}/key/{}", self.endpoint, key.0);
                let res = self.client.get(&url).send().await?;
                match res.status() {
                    StatusCode::NOT_FOUND => callback.send(Err(KeyError::AccessDenied)).whatever("callback dropped"),
                    StatusCode::OK => callback.send(Ok(key)).whatever("callback dropped"),
                    status => warn!("Unexpected status while checking key: {}", status),
                }
                res.error_for_status()?;
            }
        })
    }
}

#[derive(Default)]
struct RandomizedBackoff {
    duration: Duration,
}

impl RandomizedBackoff {
    fn next(&mut self) -> Duration {
        let low = self.duration.as_millis() as u64;
        let high = min(60_000, (low + 500) * 2);
        self.duration = Duration::from_millis(rand::thread_rng().gen_range(low, high));
        self.duration
    }

    fn reset(&mut self) {
        self.duration = Duration::default();
    }
}
