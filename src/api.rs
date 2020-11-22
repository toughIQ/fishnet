use url::Url;
use std::time::Duration;
use reqwest::StatusCode;
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
    client: reqwest::Client,
}

impl HttpApi {
    pub fn new(endpoint: Url) -> HttpApi {
        HttpApi {
            endpoint,
            client: reqwest::Client::builder()
                .user_agent(concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")))
                .timeout(Duration::from_secs(15))
                .build().expect("client")
        }
    }

    pub async fn check_key(&mut self, key: Key) -> Result<Result<Key, KeyError>, reqwest::Error> {
        let url = format!("{}/key/{}", self.endpoint, key.0);
        match self.client.get(&url).send().await {
            Ok(res) if res.status() == StatusCode::NOT_FOUND => Ok(Err(KeyError::AccessDenied)),
            Ok(res) => match res.error_for_status() {
                Ok(_) => Ok(Ok(key)),
                Err(err) => Err(err),
            }
            Err(err) => Err(err),
        }
    }
}
