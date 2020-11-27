use std::time::Duration;
use reqwest::StatusCode;
use tokio::time;
use tokio::sync::{mpsc, oneshot};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DurationSeconds, DisplayFromStr, SpaceSeparator, StringWithSeparator};
use serde_repr::Deserialize_repr as DeserializeRepr;
use shakmaty::fen::Fen;
use shakmaty::uci::Uci;
use shakmaty::variants::Variant;
use tokio_compat_02::FutureExt as _;
use crate::configure::{Endpoint, Key, KeyError};
use crate::ipc::BatchId;
use crate::logger::Logger;
use crate::util::{NevermindExt as _, RandomizedBackoff};

pub fn channel(endpoint: Endpoint, key: Option<Key>, logger: Logger) -> (ApiStub, ApiActor) {
    let (tx, rx) = mpsc::unbounded_channel();
    (ApiStub::new(tx), ApiActor::new(rx, endpoint, key, logger))
}

pub fn spawn(endpoint: Endpoint, key: Option<Key>, logger: Logger) -> ApiStub {
    let (stub, actor) = channel(endpoint, key, logger);
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
        batch_id: BatchId,
    },
    Acquire {
        query: AcquireQuery,
        callback: oneshot::Sender<Acquired>,
    },
    SubmitAnalysis {
        batch_id: BatchId,
        analysis: Vec<Option<AnalysisPart>>,
    },
    SubmitMove {
        batch_id: BatchId,
        best_move: Option<Uci>,
        callback: oneshot::Sender<Acquired>,
    }
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

#[serde_as]
#[derive(Debug, Serialize)]
struct StockfishOptions {
    #[serde_as(as = "DisplayFromStr")]
    hash: u32,
    #[serde_as(as = "DisplayFromStr")]
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
#[serde(tag = "type")]
pub enum Work {
    #[serde(rename = "analysis")]
    Analysis {
        #[serde_as(as = "DisplayFromStr")]
        id: BatchId,
    },
    Move {
        #[serde_as(as = "DisplayFromStr")]
        id: BatchId,
        level: Level,
    },
}

impl Work {
    pub fn id(&self) -> BatchId {
        match *self {
            Work::Analysis { id, .. } => id,
            Work::Move { id, .. } => id,
        }
    }
}

#[derive(DeserializeRepr, Debug)]
#[repr(u32)]
pub enum Level {
    One = 1,
    Two = 2,
    Three = 3,
    Four = 4,
    Five = 5,
    Six = 6,
    Seven = 7,
    Eight = 8,
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

#[derive(Debug, Deserialize, Copy, Clone, Eq, PartialEq)]
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

impl From<LichessVariant> for Variant {
    fn from(lichess: LichessVariant) -> Variant {
        match lichess {
            LichessVariant::Antichess => Variant::Giveaway,
            LichessVariant::Atomic => Variant::Atomic,
            LichessVariant::Chess960 | LichessVariant::Standard | LichessVariant::FromPosition => Variant::Chess,
            LichessVariant::Crazyhouse => Variant::Crazyhouse,
            LichessVariant::Horde => Variant::Horde,
            LichessVariant::KingOfTheHill => Variant::KingOfTheHill,
            LichessVariant::RacingKings => Variant::RacingKings,
            LichessVariant::ThreeCheck => Variant::ThreeCheck,
        }
    }
}

impl Default for LichessVariant {
    fn default() -> LichessVariant {
        LichessVariant::Standard
    }
}

#[must_use = "Acquired work should be processed or cancelled"]
#[derive(Debug)]
pub enum Acquired {
    Accepted(AcquireResponseBody),
    NoContent,
    BadRequest,
}

#[derive(Debug, Serialize)]
struct AnalysisRequestBody {
    fishnet: Fishnet,
    stockfish: Stockfish,
    analysis: Vec<Option<AnalysisPart>>,
}

#[serde_as]
#[derive(Debug, Serialize)]
struct MoveRequestBody {
    fishnet: Fishnet,
    stockfish: Stockfish,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(rename = "bestmove")]
    best_move: Option<Uci>,
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
        depth: u32,
        nodes: u64,
        score: Score,
        time: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        nps: Option<u32>,
    },
}

#[derive(Debug, Serialize, Copy, Clone)]
pub enum Score {
    #[serde(rename = "cp")]
    Cp(i64),
    #[serde(rename = "mate")]
    Mate(i64),
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

    pub fn abort(&mut self, batch_id: BatchId) {
        self.tx.send(ApiMessage::Abort { batch_id }).expect("api actor alive");
    }

    pub async fn acquire(&mut self, query: AcquireQuery) -> Option<Acquired> {
        let (req, res) = oneshot::channel();
        self.tx.send(ApiMessage::Acquire {
            query,
            callback: req,
        }).expect("api actor alive");
        res.await.ok()
    }

    pub fn submit_analysis(&mut self, batch_id: BatchId, analysis: Vec<Option<AnalysisPart>>) {
        self.tx.send(ApiMessage::SubmitAnalysis {
            batch_id,
            analysis,
        }).expect("api actor alive");
    }

    pub async fn submit_move_and_acquire(&mut self, batch_id: BatchId, best_move: Option<Uci>) -> Option<Acquired> {
        let (req, res) = oneshot::channel();
        self.tx.send(ApiMessage::SubmitMove {
            batch_id,
            best_move,
            callback: req,
        }).expect("api actor alive");
        res.await.ok()
    }
}

pub struct ApiActor {
    rx: mpsc::UnboundedReceiver<ApiMessage>,
    endpoint: Endpoint,
    key: Option<Key>,
    client: reqwest::Client,
    error_backoff: RandomizedBackoff,
    logger: Logger,
}

impl ApiActor {
    fn new(rx: mpsc::UnboundedReceiver<ApiMessage>, endpoint: Endpoint, key: Option<Key>, logger: Logger) -> ApiActor {
        ApiActor {
            rx,
            endpoint,
            key,
            client: reqwest::Client::builder()
                .user_agent(concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")))
                .timeout(Duration::from_secs(30))
                .pool_idle_timeout(Duration::from_secs(25))
                .build().expect("client"),
            error_backoff: RandomizedBackoff::default(),
            logger,
        }
    }

    pub async fn run(mut self) {
        self.logger.debug("Api actor started");
        while let Some(msg) = self.rx.recv().await {
            self.handle_mesage(msg).compat().await;
        }
        self.logger.debug("Api actor exited");
    }

    async fn handle_mesage(&mut self, msg: ApiMessage) {
        if let Err(err) = self.handle_message_inner(msg).await {
            match err.status() {
                Some(status) if status == StatusCode::TOO_MANY_REQUESTS => {
                    let backoff = Duration::from_secs(60) + self.error_backoff.next();
                    self.logger.error(&format!("Too many requests. Suspending requests for {:?}.", backoff));
                    time::sleep(backoff).await;
                }
                Some(status) if status.is_client_error() => {
                    let backoff = self.error_backoff.next();
                    self.logger.error(&format!("Client error: {}. Backing off {:?}.", status, backoff));
                    time::sleep(backoff).await;
                },
                Some(status) if status.is_server_error() => {
                    let backoff = self.error_backoff.next();
                    self.logger.error(&format!("Server error: {}. Backing off {:?}.", status, backoff));
                    time::sleep(backoff).await;
                }
                Some(_) => self.error_backoff.reset(),
                None => {
                    let backoff = self.error_backoff.next();
                    self.logger.error(&format!("{}. Backing off {:?}.", err, backoff));
                    time::sleep(backoff).await;
                }
            }
        } else {
            self.error_backoff.reset();
        }
    }

    async fn abort(&mut self, batch_id: BatchId) -> reqwest::Result<()> {
        Ok({
            let url = format!("{}/abort/{}", self.endpoint, batch_id);
            self.logger.warn(&format!("Aborting batch {}.", batch_id));
            self.client.post(&url).json(&VoidRequestBody {
                fishnet: Fishnet::authenticated(self.key.clone()),
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
                    StatusCode::NOT_FOUND => callback.send(Err(KeyError::AccessDenied)).nevermind("callback dropped"),
                    StatusCode::OK => callback.send(Ok(key)).nevermind("callback dropped"),
                    status => {
                        self.logger.warn(&format!("Unexpected status while checking key: {}", status));
                        res.error_for_status()?;
                    }
                }
            }
            ApiMessage::Status { callback } => {
                let url = format!("{}/status", self.endpoint);
                let res: StatusResponseBody = self.client.get(&url).send().await?.error_for_status()?.json().await?;
                callback.send(res.analysis).nevermind("callback dropped");
            }
            ApiMessage::Abort { batch_id } => {
                self.abort(batch_id).await?;
            }
            ApiMessage::Acquire { callback, query } => {
                let url = format!("{}/acquire", self.endpoint);
                let res = self.client.post(&url).query(&query).json(&VoidRequestBody {
                    fishnet: Fishnet::authenticated(self.key.clone()),
                    stockfish: Stockfish::default(),
                }).send().await?;

                match res.status() {
                    StatusCode::NO_CONTENT => callback.send(Acquired::NoContent).nevermind("callback dropped"),
                    StatusCode::BAD_REQUEST => callback.send(Acquired::BadRequest).nevermind("callback dropped"),
                    StatusCode::OK | StatusCode::ACCEPTED => {
                        if let Err(Acquired::Accepted(res)) = callback.send(Acquired::Accepted(res.json().await?)) {
                            self.logger.error(&format!("Acquired a batch, but callback dropped. Aborting."));
                            self.abort(res.work.id()).await?;
                        }
                    }
                    status => {
                        self.logger.warn(&format!("Unexpected status for acquire: {}", status));
                        res.error_for_status()?;
                    }
                }
            }
            ApiMessage::SubmitAnalysis { batch_id, analysis } => {
                let url = format!("{}/analysis/{}", self.endpoint, batch_id);
                let res = self.client.post(&url).query(&SubmitQuery {
                    stop: true,
                    slow: false,
                }).json(&AnalysisRequestBody {
                    fishnet: Fishnet::authenticated(self.key.clone()),
                    stockfish: Stockfish::default(),
                    analysis
                }).send().await?.error_for_status()?;

                if res.status() != StatusCode::NO_CONTENT {
                    self.logger.warn(&format!("Unexpected status for submitting analysis: {}", res.status()));
                }
            }
            ApiMessage::SubmitMove { batch_id, best_move, callback } => {
                let url = format!("{}/move/{}", self.endpoint, batch_id);
                let res = self.client.post(&url).json(&MoveRequestBody {
                    fishnet: Fishnet::authenticated(self.key.clone()),
                    stockfish: Stockfish::default(),
                    best_move,
                }).send().await?.error_for_status()?;

                match res.status() {
                    StatusCode::NO_CONTENT => callback.send(Acquired::NoContent).nevermind("callback dropped"),
                    StatusCode::OK | StatusCode::ACCEPTED => {
                        if let Err(Acquired::Accepted(res)) = callback.send(Acquired::Accepted(res.json().await?)) {
                            self.logger.error(&format!("Acquired a batch while submitting move, but callback dropped. Aborting."));
                            self.abort(res.work.id()).await?;
                        }
                    }
                    status => {
                        self.logger.warn(&format!("Unexpected status for submit move: {}", status));
                    }
                }
            }
        })
    }
}
