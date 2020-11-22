use url::Url;
use std::cmp::min;
use std::time::Duration;
use reqwest::StatusCode;
use tokio::time;
use tokio::time::Instant;
use tracing::{debug, error};
use crate::configure::{Key, KeyError};

struct Acquire {
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
}

pub struct HttpApi {
    endpoint: Url,
    not_before: Instant,
    backoff: Duration,
    client: reqwest::Client,
}

impl HttpApi {
    pub fn new(endpoint: Url) -> HttpApi {
        let mut api = HttpApi {
            endpoint,
            not_before: Instant::now(),
            backoff: Duration::default(),
            client: reqwest::Client::builder()
                .user_agent(concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")))
                .timeout(Duration::from_secs(15))
                .build().expect("client")
        };
        api.reset_backoff();
        api
    }

    fn reset_backoff(&mut self) {
        self.backoff = Duration::from_millis(500);
    }

    fn backoff(&mut self, base: Duration) -> Duration {
        self.backoff = min(Duration::from_secs(120), self.backoff * 2);
        self.not_before = Instant::now() + base + self.backoff;
        base + self.backoff
    }

    async fn send(&mut self, req: reqwest::Request) -> reqwest::Result<reqwest::Response> {
        time::delay_until(self.not_before).await;

        let url = req.url().clone();

        match self.client.execute(req).await {
            Ok(res) if res.status() == StatusCode::TOO_MANY_REQUESTS => {
                error!("Too many requests. Suspending requests for {:?}.", self.backoff(Duration::from_secs(60)));
                Ok(res)
            }
            Ok(res) if res.status().is_server_error() => {
                error!("Server error: {}. Backing off {:?}.", res.status(), self.backoff(Duration::default()));
                Ok(res)
            }
            Ok(res) => {
                debug!("Response: {} -> {}.", url, res.status());
                self.reset_backoff();
                Ok(res)
            }
            Err(err) => {
                error!("Network error: {}. Backing off {:?}.", err, self.backoff(Duration::default()));
                Err(err)
            }
        }
    }

    pub async fn check_key(&mut self, key: Key) -> reqwest::Result<Result<Key, KeyError>> {
        Ok({
            let url = format!("{}/key/{}", self.endpoint, key.0);
            let res = self.send(self.client.get(&url).build()?).await?;
            if res.status() == StatusCode::NOT_FOUND {
                Err(KeyError::AccessDenied)
            } else {
                res.error_for_status()?;
                Ok(key)
            }
        })
    }
}
