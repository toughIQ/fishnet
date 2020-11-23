use arrayvec::ArrayString;
use std::fmt;
use std::time::Duration;
use std::str::FromStr;
use shakmaty::fen::Fen;
use shakmaty::uci::Uci;
use shakmaty::variants::Variant;
use tokio::sync::oneshot;

/// Uniquely identifies a batch in this process.
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

/// Uniquely identifies a position within a batch.
#[derive(Debug, Clone)]
pub struct PositionId(pub usize);

#[derive(Debug, Clone)]
pub struct Skill(u32);

#[derive(Debug, Clone)]
pub struct Position {
    batch_id: BatchId,
    position_id: PositionId,

    variant: Variant,
    fen: Fen,
    moves: Vec<Uci>,
    nodes: u64,
    skill: Option<Skill>,
}

#[derive(Debug, Clone)]
pub enum Score {
    Cp(i32),
    Mate(i32),
}

#[derive(Debug, Clone)]
pub struct PositionResponse {
    pub batch_id: BatchId,
    pub position_id: PositionId,

    score: Score,
    best_move: Option<Uci>,
    pv: Vec<Uci>,
    depth: u32,
    nodes: u64,
    time: Duration,
    nps: Option<u32>,
}

#[derive(Debug)]
pub struct Pull {
    response: Option<PositionResponse>,
    next_tx: oneshot::Sender<PullResponse>,
}

#[derive(Debug)]
pub enum PullResponse {
    Position(Position),
    Sleep(Duration),
}
