use std::fmt;
use std::time::Duration;
use std::str::FromStr;
use arrayvec::ArrayString;
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

#[derive(Debug, Default, Deserialize)]
pub struct AnalysisStatus {
    pub user: QueueStatus,
    pub system: QueueStatus,
}

#[serde_as]
#[derive(Debug, Default, Deserialize)]
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
#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum Work {
    #[serde(rename = "analysis")]
    Analysis {
        #[serde_as(as = "DisplayFromStr")]
        id: BatchId,
        #[serde(default)]
        nodes: Option<NodeLimit>,
    },
    #[serde(rename = "move")]
    Move {
        #[serde_as(as = "DisplayFromStr")]
        id: BatchId,
        level: SkillLevel,
        #[serde(default)]
        clock: Option<Clock>,
    },
}

impl Work {
    pub fn id(&self) -> BatchId {
        match *self {
            Work::Analysis { id, .. } => id,
            Work::Move { id, .. } => id,
        }
    }

    pub fn is_analysis(&self) -> bool {
        matches!(self, Work::Analysis { .. })
    }
}

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub struct BatchId(ArrayString<[u8; 16]>);

impl FromStr for BatchId {
    type Err = arrayvec::CapacityError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(BatchId(s.parse()?))
    }
}

impl fmt::Display for BatchId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

#[derive(Debug, Copy, Clone, Deserialize)]
pub struct NodeLimit(pub u64);

impl Default for NodeLimit {
    fn default() -> NodeLimit {
        NodeLimit(2_500_000)
    }
}

#[derive(DeserializeRepr, Debug, Copy, Clone)]
#[repr(u32)]
pub enum SkillLevel {
    One = 1,
    Two = 2,
    Three = 3,
    Four = 4,
    Five = 5,
    Six = 6,
    Seven = 7,
    Eight = 8,
}

impl SkillLevel {
    pub fn time(self) -> Duration {
        use SkillLevel::*;
        Duration::from_millis(match self {
            One => 50,
            Two => 100,
            Three => 150,
            Four => 200,
            Five => 300,
            Six => 400,
            Seven => 500,
            Eight => 1000,
        })
    }

    pub fn elo(self) -> u32 {
        use SkillLevel::*;
        match self {
            One => 800,
            Two => 1100,
            Three => 1400,
            Four => 1700,
            Five => 2000,
            Six => 2300,
            Seven => 2700,
            Eight => 3000,
        }
    }

    pub fn depth(self) -> u32 {
        use SkillLevel::*;
        match self {
            One | Two | Three | Four | Five => 5,
            Six => 8,
            Seven => 13,
            Eight => 22,
        }
    }
}

#[serde_as]
#[derive(Debug, Deserialize, Clone)]
pub struct Clock {
    pub wtime: Centis,
    pub btime: Centis,
    #[serde_as(as = "DurationSeconds<u64>")]
    pub inc: Duration,
}

#[derive(Debug, Copy, Clone, Deserialize)]
pub struct Centis(u32);

impl From<Centis> for Duration {
    fn from(Centis(centis): Centis) -> Duration {
        Duration::from_millis(u64::from(centis) * 10)
    }
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

#[derive(Debug, Serialize)]
struct MoveRequestBody {
    fishnet: Fishnet,
    #[serde(rename = "move")]
    m: BestMove,
}

#[serde_as]
#[derive(Debug, Serialize)]
struct BestMove {
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
            if err.status().map_or(false, |s| s.is_success()) {
                self.error_backoff.reset();
            } else if err.status() == Some(StatusCode::TOO_MANY_REQUESTS) {
                let backoff = Duration::from_secs(60) + self.error_backoff.next();
                self.logger.error(&format!("Too many requests. Suspending requests for {:?}.", backoff));
                time::sleep(backoff).await;
            } else {
                let backoff = self.error_backoff.next();
                self.logger.error(&format!("{}. Backing off {:?}.", err, backoff));
                time::sleep(backoff).await;
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
                let res = self.client.get(&url).send().await?;
                match res.status() {
                    StatusCode::OK => callback.send(res.json::<StatusResponseBody>().await?.analysis).nevermind("callback dropped"),
                    StatusCode::NOT_FOUND => callback.send(AnalysisStatus::default()).nevermind("callback dropped"),
                    status => {
                        self.logger.warn(&format!("Unexpected status for queue status: {}", status));
                        res.error_for_status()?;
                    }
                }
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
                    m: BestMove {
                        best_move,
                    },
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
