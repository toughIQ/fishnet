use std::time::Duration;
use shakmaty::fen::Fen;
use shakmaty::uci::Uci;
use shakmaty::variants::Variant;
use tokio::sync::oneshot;

#[derive(Debug)]
pub struct WorkId(u64);

#[derive(Debug)]
pub struct WorkUnitId(u64);

#[derive(Debug)]
pub struct Skill(u32);

#[derive(Debug)]
pub struct WorkUnit {
    work_id: WorkId,
    work_unit_id: WorkUnitId,

    variant: Variant,
    fen: Fen,
    moves: Vec<Uci>,
    nodes: u64,
    skill: Option<Skill>,
}

#[derive(Debug)]
pub enum Score {
    Cp(i32),
    Mate(i32),
}

#[derive(Debug)]
pub struct WorkUnitResponse {
    work_id: WorkId,
    work_unit_id: WorkUnitId,

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
    response: Option<WorkUnitResponse>,
    next_tx: oneshot::Sender<PullResponse>,
}

#[derive(Debug)]
pub enum PullResponse {
    WorkUnit(WorkUnit),
    Sleep(Duration),
}
