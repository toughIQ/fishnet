use url::Url;
use std::time::Duration;
use shakmaty::fen::Fen;
use shakmaty::uci::Uci;
use tokio::sync::oneshot;
use crate::api::{Score, LichessVariant, Work, BatchId};
use crate::assets::EngineFlavor;

/// Uniquely identifies a position within a batch.
#[derive(Debug, Copy, Clone)]
pub struct PositionId(pub usize);

#[derive(Debug, Clone)]
pub struct Position {
    pub work: Work,
    pub position_id: PositionId,
    pub flavor: EngineFlavor,
    pub url: Option<Url>,

    pub variant: LichessVariant,
    pub chess960: bool,
    pub fen: Fen,
    pub moves: Vec<Uci>,
}

#[derive(Debug, Clone)]
pub struct PositionResponse {
    pub work: Work,
    pub position_id: PositionId,
    pub url: Option<Url>,

    pub score: Score,
    pub best_move: Option<Uci>,
    pub pv: Vec<Uci>,
    pub depth: u32,
    pub nodes: u64,
    pub time: Duration,
    pub nps: Option<u32>,
}

#[derive(Debug)]
pub struct PositionFailed {
    pub batch_id: BatchId,
}

#[derive(Debug)]
pub struct Pull {
    pub response: Option<Result<PositionResponse, PositionFailed>>,
    pub callback: oneshot::Sender<Position>,
}

impl Pull {
    pub fn split(self) -> (Option<Result<PositionResponse, PositionFailed>>, oneshot::Sender<Position>) {
        (self.response, self.callback)
    }
}
