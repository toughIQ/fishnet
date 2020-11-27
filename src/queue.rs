use std::cmp::{min, max};
use std::convert::TryInto;
use std::collections::{VecDeque, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};
use shakmaty::uci::Uci;
use url::Url;
use tokio::sync::{mpsc, oneshot, Mutex, Notify};
use tokio::time;
use crate::api::{BatchId, Work, AcquireQuery, AcquireResponseBody, Acquired, ApiStub, AnalysisPart};
use crate::configure::{BacklogOpt, Endpoint};
use crate::ipc::{Position, PositionResponse, PositionFailed, PositionId, Pull};
use crate::logger::{Logger, ProgressAt, QueueStatusBar};
use crate::util::{NevermindExt as _, RandomizedBackoff};

pub fn channel(endpoint: Endpoint, opt: BacklogOpt, cores: usize, api: ApiStub, logger: Logger) -> (QueueStub, QueueActor) {
    let state = Arc::new(Mutex::new(QueueState::new(cores, logger.clone())));
    let (tx, rx) = mpsc::unbounded_channel();
    let interrupt = Arc::new(Notify::new());
    (QueueStub::new(tx, interrupt.clone(), state.clone(), api.clone()), QueueActor::new(rx, interrupt, state, endpoint, opt, api, logger))
}

#[derive(Clone)]
pub struct QueueStub {
    tx: Option<mpsc::UnboundedSender<QueueMessage>>,
    interrupt: Arc<Notify>,
    state: Arc<Mutex<QueueState>>,
    api: ApiStub,
}

impl QueueStub {
    fn new(tx: mpsc::UnboundedSender<QueueMessage>, interrupt: Arc<Notify>, state: Arc<Mutex<QueueState>>, api: ApiStub) -> QueueStub {
        QueueStub {
            tx: Some(tx),
            interrupt,
            state,
            api,
        }
    }

    pub async fn pull(&mut self, pull: Pull) {
        let mut state = self.state.lock().await;
        let (response, callback) = pull.split();
        if let Some(response) = response {
            state.handle_position_response(self.clone(), response);
        }
        if let Err(callback) = state.try_pull(callback) {
            if let Some(ref mut tx) = self.tx {
                tx.send(QueueMessage::Pull {
                    callback,
                }).nevermind("queue dropped");
            }
        }
    }

    fn submit_move(&mut self, batch_id: BatchId, best_move: Option<Uci>) {
        if let Some(ref tx) = self.tx {
            tx.send(QueueMessage::SubmitMove {
                batch_id,
                best_move,
            }).nevermind("moves are too short lived to abort anyway");

            // Submitting a move can generate a follow-up response. So skip
            // the queue backoff.
            // TODO: Skipp multiple queue entries.
            self.interrupt.notify_one();
        }
    }

    pub async fn shutdown_soon(&mut self) {
        let mut state = self.state.lock().await;
        state.shutdown_soon = true;
        self.tx.take();
        self.interrupt.notify_one();
    }

    pub async fn shutdown(mut self) {
        self.shutdown_soon().await;

        let mut state = self.state.lock().await;
        for (k, _) in state.pending.drain() {
            self.api.abort(k);
        }
    }

    pub async fn stats(&self) -> StatsRecorder {
        let state = self.state.lock().await;
        state.stats.clone()
    }
}

struct QueueState {
    shutdown_soon: bool,
    cores: usize,
    incoming: VecDeque<Position>,
    pending: HashMap<BatchId, PendingBatch>,
    stats: StatsRecorder,
    logger: Logger,
}

impl QueueState {
    fn new(cores: usize, logger: Logger) -> QueueState {
        QueueState {
            shutdown_soon: false,
            cores,
            incoming: VecDeque::new(),
            pending: HashMap::new(),
            stats: StatsRecorder::new(),
            logger,
        }
    }

    fn status_bar(&self) -> QueueStatusBar {
        QueueStatusBar {
            pending: self.pending.values().map(|p| p.pending()).sum(),
            cores: self.cores,
        }
    }

    fn add_incoming_batch(&mut self, batch: IncomingBatch) {
        let batch_id = batch.work.id();
        if self.pending.contains_key(&batch_id) {
            self.logger.error(&format!("Dropping duplicate incoming batch {}", batch_id));
        } else {
            let progress_at = ProgressAt::from(&batch);

            // Reversal only for cosmetics when displaying progress.
            let mut positions = Vec::with_capacity(batch.positions.len());
            for pos in batch.positions.into_iter().rev() {
                positions.insert(0, match pos {
                    Skip::Present(pos) => {
                        self.incoming.push_back(pos);
                        None
                    }
                    Skip::Skip => Some(Skip::Skip),
                });
            }

            self.pending.insert(batch_id, PendingBatch {
                work: batch.work,
                positions,
                url: batch.url,
                started_at: Instant::now(),
            });

            self.logger.progress(self.status_bar(), progress_at);
        }
    }

    fn handle_position_response(&mut self, mut queue: QueueStub, res: Result<PositionResponse, PositionFailed>) {
        match res {
            Ok(res) => {
                let progress_at = ProgressAt::from(&res);
                let batch_id = res.work.id();
                if let Some(pending) = self.pending.get_mut(&batch_id) {
                    if let Some(pos) = pending.positions.get_mut(res.position_id.0) {
                        *pos = Some(Skip::Present(res));
                    }
                }
                self.logger.progress(self.status_bar(), progress_at);
                self.maybe_finished(queue, batch_id);
            }
            Err(failed) => {
                self.pending.remove(&failed.batch_id);
                self.incoming.retain(|p| p.work.id() != failed.batch_id);
                queue.api.abort(failed.batch_id);
            }
        }
    }

    fn try_pull(&mut self, callback: oneshot::Sender<Position>) -> Result<(), oneshot::Sender<Position>> {
        if let Some(position) = self.incoming.pop_front() {
            if let Err(err) = callback.send(position) {
                self.incoming.push_front(err);
            }
            Ok(())
        } else {
            Err(callback)
        }
    }

    fn maybe_finished(&mut self, mut queue: QueueStub, batch: BatchId) {
        if let Some(pending) = self.pending.remove(&batch) {
            match pending.try_into_completed() {
                Ok(completed) => {
                    let nps_string = match completed.nps() {
                        Some(nps) => {
                            self.stats.record_batch(completed.total_positions(), completed.total_nodes(), nps);
                            nps.to_string()
                        }
                        None => "?".to_owned(),
                    };
                    match completed.url {
                        Some(ref url) => {
                            self.logger.info(&format!("{} {} finished ({} nps)", self.status_bar(), url, nps_string));
                        }
                        None => {
                            self.logger.info(&format!("{} {} finished ({} nps)", self.status_bar(), batch, nps_string));
                        }
                    }
                    match completed.work {
                        Work::Analysis { id } => queue.api.submit_analysis(id, completed.into_analysis()),
                        Work::Move { id, .. } => queue.submit_move(id, completed.into_best_move()),
                    }
                }
                Err(pending) => {
                    let progress_report = pending.progress_report();
                    if progress_report.iter().filter(|p| p.is_some()).count() % (self.cores * 2) == 0 {
                        queue.api.submit_analysis(pending.work.id(), progress_report);
                    }

                    self.pending.insert(pending.work.id(), pending);
                }
            }
        }
    }
}

#[derive(Debug)]
enum QueueMessage {
    Pull {
        callback: oneshot::Sender<Position>,
    },
    SubmitMove {
        batch_id: BatchId,
        best_move: Option<Uci>,
    },
}

pub struct QueueActor {
    rx: mpsc::UnboundedReceiver<QueueMessage>,
    interrupt: Arc<Notify>,
    state: Arc<Mutex<QueueState>>,
    api: ApiStub,
    endpoint: Endpoint,
    opt: BacklogOpt,
    backoff: RandomizedBackoff,
    logger: Logger,
}

impl QueueActor {
    fn new(rx: mpsc::UnboundedReceiver<QueueMessage>, interrupt: Arc<Notify>, state: Arc<Mutex<QueueState>>, endpoint: Endpoint, opt: BacklogOpt, api: ApiStub, logger: Logger) -> QueueActor {
        QueueActor {
            rx,
            interrupt,
            state,
            api,
            endpoint,
            opt,
            backoff: RandomizedBackoff::default(),
            logger,
        }
    }

    pub async fn run(self) {
        self.logger.debug("Queue actor started");
        self.run_inner().await;
    }

    pub async fn backlog_wait_time(&mut self) -> (Duration, AcquireQuery) {
        let sec = Duration::from_secs(1);
        let min_user_backlog = {
            let state = self.state.lock().await;
            state.stats.min_user_backlog()
        };
        let user_backlog = max(self.opt.user.map_or(Duration::default(), Duration::from), min_user_backlog);
        let system_backlog = self.opt.system.map_or(Duration::default(), Duration::from);

        if user_backlog >= sec || system_backlog >= sec {
            if let Some(status) = self.api.status().await {
                let user_wait = user_backlog.checked_sub(status.user.oldest).unwrap_or(Duration::default());
                let system_wait = system_backlog.checked_sub(status.system.oldest).unwrap_or(Duration::default());
                self.logger.debug(&format!("User wait: {:?} due to {:?} for oldest {:?}, system wait: {:?} due to {:?} for oldest {:?}",
                       user_wait, user_backlog, status.user.oldest,
                       system_wait, system_backlog, status.system.oldest));
                let slow = user_wait >= system_wait + sec;
                return (min(user_wait, system_wait), AcquireQuery { slow });
            }
        }

        let slow = min_user_backlog >= sec;
        (Duration::default(), AcquireQuery { slow })
    }

    async fn handle_acquired_response_body(&mut self, body: AcquireResponseBody) {
        match IncomingBatch::from_acquired(self.endpoint.clone(), body) {
            Ok(incoming) => {
                let mut state = self.state.lock().await;
                state.add_incoming_batch(incoming);
            }
            Err(completed) => {
                let batch_id = completed.work.id();
                self.logger.warn(&format!("Completed empty batch {}.", batch_id));
                self.api.submit_analysis(batch_id, completed.into_analysis());
            }
        }
    }

    async fn run_inner(mut self) {
        while let Some(msg) = self.rx.recv().await {
            match msg {
                QueueMessage::Pull { mut callback } => {
                    loop {
                        {
                            let mut state = self.state.lock().await;
                            callback = match state.try_pull(callback) {
                                Ok(()) => break,
                                Err(not_done) => not_done,
                            };

                            if state.shutdown_soon {
                                break;
                            }
                        }

                        let (wait, query) = tokio::select! {
                            _ = callback.closed() => break,
                            res = self.backlog_wait_time() => res,
                        };

                        if wait >= Duration::from_secs(60) {
                            self.logger.info(&format!("Going idle for {:?}.", wait));
                        } else if wait >= Duration::from_secs(1) {
                            self.logger.debug(&format!("Going idle for {:?}.", wait));
                        }

                        tokio::select! {
                            _ = callback.closed() => break,
                            _ = self.interrupt.notified() => continue,
                            _ = time::sleep(wait) => (),
                        }

                        match self.api.acquire(query).await {
                            Some(Acquired::Accepted(body)) => {
                                self.backoff.reset();
                                self.handle_acquired_response_body(body).await;
                            }
                            Some(Acquired::NoContent) => {
                                let backoff = self.backoff.next();
                                self.logger.debug(&format!("No job received. Backing off {:?}.", backoff));
                                tokio::select! {
                                    _ = callback.closed() => break,
                                    _ = self.interrupt.notified() => (),
                                    _ = time::sleep(backoff) => (),
                                }
                            }
                            Some(Acquired::BadRequest) => {
                                self.logger.error("Client update might be required. Stopping queue");
                                let mut state = self.state.lock().await;
                                state.shutdown_soon = true;
                            },
                            None => (),
                        }
                    }
                }
                QueueMessage::SubmitMove { batch_id, best_move } => {
                    // TODO: Submit move
                }
            }
        }

    }
}

impl Drop for QueueActor {
    fn drop(&mut self) {
        self.logger.debug("Queue actor exited");
    }
}

#[derive(Debug, Clone)]
enum Skip<T> {
    Present(T),
    Skip,
}

impl<T> Skip<T> {
    fn is_skipped(&self) -> bool {
        matches!(self, Skip::Skip)
    }
}

#[derive(Debug, Clone)]
pub struct IncomingBatch {
    work: Work,
    positions: Vec<Skip<Position>>,
    url: Option<Url>,
}

impl IncomingBatch {
    fn from_acquired(endpoint: Endpoint, body: AcquireResponseBody) -> Result<IncomingBatch, CompletedBatch> {
        let url = body.game_id.as_ref().map(|g| {
            let mut url = endpoint.url.clone();
            url.set_path(g);
            url
        });

        let nodes = body.nodes.unwrap_or(4_000_000);

        Ok(IncomingBatch {
            work: body.work.clone(),
            url: url.clone(),
            positions: match body.work {
                Work::Move { .. } => {
                    vec![Skip::Present(Position {
                        work: body.work,
                        position_id: PositionId(0),
                        url,
                        variant: body.variant,
                        fen: body.position,
                        moves: body.moves,
                        nodes,
                    })]
                }
                Work::Analysis { .. } => {
                    let mut moves = Vec::new();
                    let mut positions = vec![Skip::Present(Position {
                        work: body.work.clone(),
                        position_id: PositionId(0),
                        url: url.clone().map(|mut url| {
                            url.set_fragment(Some("0"));
                            url
                        }),
                        variant: body.variant,
                        fen: body.position.clone(),
                        moves: moves.clone(),
                        nodes,
                    })];

                    for (i, m) in body.moves.into_iter().enumerate() {
                        let mut url = endpoint.url.clone();
                        moves.push(m);
                        positions.push(Skip::Present(Position {
                            work: body.work.clone(),
                            position_id: PositionId(1 + i),
                            url: body.game_id.as_ref().map(|g| {
                                url.set_path(g);
                                url.set_fragment(Some(&(1 + i).to_string()));
                                url
                            }),
                            variant: body.variant,
                            fen: body.position.clone(),
                            moves: moves.clone(),
                            nodes,
                        }));
                    }

                    for skip in body.skip_positions.into_iter() {
                        if let Some(pos) = positions.get_mut(skip) {
                            *pos = Skip::Skip;
                        }
                    }

                    // Edge case: Batch is immediately completed, because all
                    // positions are skipped.
                    if positions.iter().all(Skip::is_skipped) {
                        let now = Instant::now();
                        return Err(CompletedBatch {
                            work: body.work.clone(),
                            url,
                            started_at: now,
                            completed_at: now,
                            positions: positions.into_iter().map(|_| Skip::Skip).collect(),
                        });
                    }

                    positions
                }
            }
        })
    }
}

impl From<&IncomingBatch> for ProgressAt {
    fn from(batch: &IncomingBatch) -> ProgressAt {
        ProgressAt {
            batch_id: batch.work.id(),
            batch_url: batch.url.clone(),
            position_id: None,
        }
    }
}

#[derive(Debug, Clone)]
struct PendingBatch {
    work: Work,
    positions: Vec<Option<Skip<PositionResponse>>>,
    url: Option<Url>,
    started_at: Instant,
}

impl PendingBatch {
    fn try_into_completed(self) -> Result<CompletedBatch, PendingBatch> {
        match self.positions.clone().into_iter().collect() {
            Some(positions) => Ok(CompletedBatch {
                work: self.work,
                positions,
                url: self.url,
                started_at: self.started_at,
                completed_at: Instant::now(),
            }),
            None => Err(self),
        }
    }

    fn progress_report(&self) -> Vec<Option<AnalysisPart>> {
        self.positions.iter().enumerate().map(|(i, p)| match p {
            // Quirk: Lila distinguishes progress reports from complete
            // analysis by looking at the first part.
            Some(Skip::Present(pos)) if i > 0 => Some(AnalysisPart::Complete {
                pv: pos.pv.clone(),
                depth: pos.depth,
                score: pos.score,
                time: pos.time.as_millis() as u64,
                nodes: pos.nodes,
                nps: pos.nps,
            }),
            _ => None,
        }).collect()
    }

    fn pending(&self) -> usize {
        self.positions.iter().filter(|p| p.is_none()).count()
    }
}

pub struct CompletedBatch {
    work: Work,
    positions: Vec<Skip<PositionResponse>>,
    url: Option<Url>,
    started_at: Instant,
    completed_at: Instant,
}

impl CompletedBatch {
    fn into_analysis(self) -> Vec<Option<AnalysisPart>> {
        self.positions.into_iter().map(|p| {
            Some(match p {
                Skip::Skip => AnalysisPart::Skipped {
                    skipped: true,
                },
                Skip::Present(pos) => AnalysisPart::Complete {
                    pv: pos.pv,
                    depth: pos.depth,
                    score: pos.score,
                    time: pos.time.as_millis() as u64,
                    nodes: pos.nodes,
                    nps: pos.nps,
                },
            })
        }).collect()
    }

    fn into_best_move(self) -> Option<Uci> {
        self.positions.into_iter().next().and_then(|p| match p {
            Skip::Skip => None,
            Skip::Present(pos) => pos.best_move,
        })
    }

    fn total_positions(&self) -> u64 {
        self.positions.iter().map(|p| match p {
            Skip::Skip => 0,
            Skip::Present(_) => 1,
        }).sum()
    }

    fn total_nodes(&self) -> u64 {
        self.positions.iter().map(|p| match p {
            Skip::Skip => 0,
            Skip::Present(pos) => pos.nodes,
        }).sum()
    }

    fn nps(&self) -> Option<u32> {
        self.completed_at.checked_duration_since(self.started_at).and_then(|time| {
            self.total_nodes().checked_div(time.as_secs())
        }).and_then(|nps| nps.try_into().ok())
    }
}

#[derive(Clone)]
pub struct StatsRecorder {
    pub total_batches: u64,
    pub total_positions: u64,
    pub total_nodes: u64,
    nps: u32,
}

impl StatsRecorder {
    fn new() -> StatsRecorder {
        StatsRecorder {
            total_batches: 0,
            total_positions: 0,
            total_nodes: 0,
            nps: 1_500_000, // start low
        }
    }

    fn record_batch(&mut self, positions: u64, nodes: u64, nps: u32) {
        self.total_batches += 1;
        self.total_positions += positions;
        self.total_nodes += nodes;

        let alpha = 0.8;
        self.nps = max(1, (f64::from(self.nps) * alpha + f64::from(nps) * (1.0 - alpha)) as u32);
    }

    fn min_user_backlog(&self) -> Duration {
        // The average batch has 60 positions, analysed with 4_000_000 nodes
        // each. Top end clients take no longer than 60 seconds.
        let best_batch_seconds = 60;

        // Estimate how long this client would take for the next batch,
        // capped at timeout.
        let estimated_batch_seconds = u64::from(min(6 * 60, 60 * 4_000_000 / self.nps));

        // Its worth joining if queue wait time + estimated time < top client
        // time on empty queue.
        Duration::from_secs(estimated_batch_seconds.saturating_sub(best_batch_seconds))
    }
}
