use url::Url;
use std::cmp::min;
use std::time::Duration;
use reqwest::StatusCode;
use tokio::time;
use tokio::sync::{mpsc, oneshot};
use tracing::{warn, error};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DurationSeconds, DisplayFromStr, SpaceSeparator, StringWithSeparator};
use shakmaty::fen::Fen;
use shakmaty::uci::Uci;
use crate::configure::{Key, KeyError};
use crate::ipc::BatchId;
use crate::util::WhateverExt as _;

pub fn channel(endpoint: Url) -> (ApiStub, ApiActor) {
    let (tx, rx) = mpsc::unbounded_channel();
    (ApiStub::new(tx), ApiActor::new(rx, endpoint))
}

pub fn spawn(endpoint: Url) -> ApiStub {
    let (stub, actor) = channel(endpoint);
    tokio::spawn(async move {
        actor.run().await;
    });
    stub
}

#[derive(Debug)]
enum ApiMessage {
    CheckKey {
        key: Key,
        callback: oneshot::Sender<Result<Key, KeyError>>,
    },
    Status {
        callback: oneshot::Sender<AnalysisStatus>,
    },
    Abort {
        key: Option<Key>,
        batch_id: BatchId,
    },
    Acquire {
        key: Option<Key>,
        query: AcquireQuery,
        callback: oneshot::Sender<Acquired>,
    },
    SubmitAnalysis {
        batch_id: BatchId,
        body: AnalysisRequestBody,
    },
}

#[derive(Debug, Deserialize)]
struct StatusResponseBody {
    analysis: AnalysisStatus,
}

#[derive(Debug, Deserialize)]
pub struct AnalysisStatus {
    pub user: QueueStatus,
    pub system: QueueStatus,
}

#[serde_as]
#[derive(Debug, Deserialize)]
pub struct QueueStatus {
    pub acquired: u64,
    pub queued: u64,
    #[serde_as(as = "DurationSeconds<u64>")]
    pub oldest: Duration,
}

#[derive(Debug, Serialize)]
pub struct VoidRequestBody {
    fishnet: Fishnet,
    stockfish: Stockfish,
}

#[derive(Debug, Serialize)]
struct Fishnet {
    version: &'static str,
    python: &'static str,
    apikey: String,
}

impl Fishnet {
    fn authenticated(key: Option<Key>) -> Fishnet {
        Fishnet {
            version: env!("CARGO_PKG_VERSION"),
            python: "-",
            apikey: key.map_or("".to_owned(), |k| k.0),
        }
    }
}

#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize)]
pub struct AcquireQuery {
    pub slow: bool,
}

#[serde_as]
#[derive(Debug, Deserialize)]
pub struct Work {
    #[serde(rename = "type")]
    pub work_type: WorkType,
    #[serde_as(as = "DisplayFromStr")]
    pub id: BatchId,
}

#[derive(Debug, Deserialize)]
pub enum WorkType {
    #[serde(rename = "analysis")]
    Analysis,
    #[serde(rename = "move")]
    Move,
}

#[serde_as]
#[derive(Debug, Deserialize)]
pub struct AcquireResponseBody {
    pub work: Work,
    #[serde(default)]
    pub game_id: Option<String>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub position: Option<Fen>,
    #[serde(default)]
    pub variant: LichessVariant,
    #[serde_as(as = "StringWithSeparator::<SpaceSeparator, Uci>")]
    pub moves: Vec<Uci>,
    #[serde(default)]
    pub nodes: Option<u64>,
    #[serde(rename = "skipPositions", default)]
    pub skip_positions: Vec<usize>,
}

#[derive(Debug, Deserialize)]
pub enum LichessVariant {
    #[serde(rename = "antichess")]
    Antichess,
    #[serde(rename = "atomic")]
    Atomic,
    #[serde(rename = "chess960")]
    Chess960,
    #[serde(rename = "crazyhouse")]
    Crazyhouse,
    #[serde(rename = "fromPosition")]
    FromPosition,
    #[serde(rename = "horde")]
    Horde,
    #[serde(rename = "kingOfTheHill")]
    KingOfTheHill,
    #[serde(rename = "racingKings")]
    RacingKings,
    #[serde(rename = "standard")]
    Standard,
    #[serde(rename = "threeCheck")]
    ThreeCheck,
}

impl Default for LichessVariant {
    fn default() -> LichessVariant {
        LichessVariant::Standard
    }
}

#[derive(Debug)]
pub enum Acquired {
    Accepted(AcquireResponseBody),
    NoContent,
}

#[derive(Debug, Serialize)]
pub struct AnalysisRequestBody {
    fishnet: Fishnet,
    stockfish: Stockfish,
    analysis: Vec<AnalysisPart>,
}

#[serde_as]
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum AnalysisPart {
    Skipped {
        skipped: bool,
    },
    Complete {
        #[serde_as(as = "StringWithSeparator::<SpaceSeparator, Uci>")]
        #[serde(skip_serializing_if = "Vec::is_empty")]
        pv: Vec<Uci>,
        depth: u64,
        score: Score,
        #[serde(skip_serializing_if = "Option::is_none")]
        time: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        nodes: Option<u64>,
        nps: Option<u64>,
    },
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum Score {
    #[serde(rename = "cp")]
    Cp(i64),
    #[serde(rename = "mate")]
    Mate(i64),
}

#[serde_as]
#[derive(Debug, Serialize)]
pub struct MoveRequestBody {
    fishnet: Fishnet,
    stockfish: Stockfish,
    #[serde_as(as = "Option<DisplayFromStr>")]
    bestmove: Option<Uci>,
}

#[derive(Debug, Serialize)]
struct SubmitQuery {
    slow: bool,
    stop: bool,
}

#[derive(Debug, Clone)]
pub struct ApiStub {
    tx: mpsc::UnboundedSender<ApiMessage>,
}

impl ApiStub {
    fn new(tx: mpsc::UnboundedSender<ApiMessage>) -> ApiStub {
        ApiStub { tx }
    }

    pub async fn check_key(&mut self, key: Key) -> Option<Result<Key, KeyError>> {
        let (req, res) = oneshot::channel();
        self.tx.send(ApiMessage::CheckKey {
            key,
            callback: req,
        }).expect("api actor alive");
        res.await.ok()
    }

    pub async fn status(&mut self) -> Option<AnalysisStatus> {
        let (req, res) = oneshot::channel();
        self.tx.send(ApiMessage::Status {
            callback: req,
        }).expect("api actor alive");
        res.await.ok()
    }

    pub fn abort(&mut self, key: Option<Key>, batch_id: BatchId) {
        self.tx.send(ApiMessage::Abort {
            key,
            batch_id,
        }).expect("api actor alive");
    }

    pub async fn acquire(&mut self, key: Option<Key>, query: AcquireQuery) -> Option<Acquired> {
        let (req, res) = oneshot::channel();
        self.tx.send(ApiMessage::Acquire {
            key,
            query,
            callback: req,
        }).expect("api actor alive");
        res.await.ok()
    }

    pub fn submit_analysis(&mut self, batch_id: BatchId, body: AnalysisRequestBody) {
        self.tx.send(ApiMessage::SubmitAnalysis {
            batch_id,
            body,
        }).expect("api actor alive");
    }
}

pub struct ApiActor {
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

    pub async fn run(mut self) {
        while let Some(msg) = self.rx.recv().await {
            self.handle_mesage(msg).await;
        }
    }

    async fn handle_mesage(&mut self, msg: ApiMessage) {
        if let Err(err) = self.handle_message_inner(msg).await {
            match err.status() {
                Some(status) if status == StatusCode::TOO_MANY_REQUESTS => {
                    let backoff = Duration::from_secs(60) + self.error_backoff.next();
                    error!("Too many requests. Suspending requests for {:?}.", backoff);
                    time::delay_for(backoff).await;
                }
                Some(status) if status == StatusCode::BAD_REQUEST => {
                    error!("Client error: {}", err);
                },
                Some(status) if status.is_server_error() => {
                    let backoff = self.error_backoff.next();
                    error!("Server error: {}. Backing off {:?}.", status, backoff);
                    time::delay_for(backoff).await;
                }
                Some(_) => self.error_backoff.reset(),
                None => {
                    let backoff = self.error_backoff.next();
                    error!("{}. Backing off {:?}.", err, backoff);
                    time::delay_for(backoff).await;
                }
            }
        } else {
            self.error_backoff.reset();
        }
    }

    async fn abort(&mut self, key: Option<Key>, batch_id: BatchId) -> reqwest::Result<()> {
        Ok({
            let url = format!("{}/abort/{}", self.endpoint, batch_id);
            warn!("Aborting batch {}.", batch_id);
            self.client.post(&url).json(&VoidRequestBody {
                fishnet: Fishnet::authenticated(key),
                stockfish: Stockfish::default(),
            }).send().await?.error_for_status()?;
        })
    }

    async fn handle_message_inner(&mut self, msg: ApiMessage) -> reqwest::Result<()> {
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
            ApiMessage::Status { callback } => {
                let url = format!("{}/status", self.endpoint);
                let res: StatusResponseBody = self.client.get(&url).send().await?.error_for_status()?.json().await?;
                callback.send(res.analysis).whatever("callback dropped");
            }
            ApiMessage::Abort { key, batch_id } => {
                self.abort(key, batch_id).await?;
            }
            ApiMessage::Acquire { key, callback, query } => {
                let url = format!("{}/acquire", self.endpoint);
                let res = self.client.post(&url).query(&query).json(&VoidRequestBody {
                    fishnet: Fishnet::authenticated(key.clone()),
                    stockfish: Stockfish::default(),
                }).send().await?.error_for_status()?;

                match res.status() {
                    StatusCode::NO_CONTENT => callback.send(Acquired::NoContent).whatever("callback dropped"),
                    StatusCode::OK | StatusCode::ACCEPTED => {
                        if let Err(Acquired::Accepted(res)) = callback.send(Acquired::Accepted(res.json().await?)) {
                            error!("Acquired a batch, but callback dropped. Aborting.");
                            self.abort(key, res.work.id).await?;
                        }
                    }
                    status => warn!("Unexpected status for acquire: {}", status),
                }
            }
            ApiMessage::SubmitAnalysis { batch_id, body } => {
                let url = format!("{}/analyis/{}", self.endpoint, batch_id);
                let res = self.client.post(&url).query(&SubmitQuery {
                    stop: true,
                    slow: false,
                }).json(&body).send().await?.error_for_status()?;

                if res.status() != StatusCode::NO_CONTENT {
                    warn!("Unexpected status for submitting analysis: {}", res.status());
                }
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
