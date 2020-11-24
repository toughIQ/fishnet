use std::cmp::min;
use std::collections::{VecDeque, HashMap};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, Mutex, Notify};
use tokio::time;
use tracing::{debug, warn, info};
use crate::api::{AcquireQuery, AcquireResponseBody, Acquired, ApiStub, AnalysisPart};
use crate::configure::BacklogOpt;
use crate::ipc::{BatchId, Position, PositionResponse, PositionId, Pull};
use crate::util::{NevermindExt as _, RandomizedBackoff};

pub fn channel(opt: BacklogOpt, api: ApiStub) -> (QueueStub, QueueActor) {
    let state = Arc::new(Mutex::new(QueueState::new()));
    let (tx, rx) = mpsc::unbounded_channel();
    let interrupt = Arc::new(Notify::new());
    (QueueStub::new(tx, interrupt.clone(), state.clone(), api.clone()), QueueActor::new(rx, interrupt, state, opt, api))
}

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
        if let Err(pull) = state.respond(&mut self.api, pull) {
            if let Some(ref mut tx) = self.tx {
                tx.send(QueueMessage::Pull {
                    callback: pull.callback,
                }).nevermind("queue dropped");
            }
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
}

struct QueueState {
    shutdown_soon: bool,
    incoming: VecDeque<Position>,
    pending: HashMap<BatchId, PendingBatch>,
}

impl QueueState {
    fn new() -> QueueState {
        QueueState {
            shutdown_soon: false,
            incoming: VecDeque::new(),
            pending: HashMap::new(),
        }
    }

    fn add_incoming_batch(&mut self, api: &mut ApiStub, batch: IncomingBatch) {
        let mut positions = Vec::with_capacity(batch.positions.len());

        for pos in batch.positions {
            match pos {
                Skip::Present(pos) => {
                    self.incoming.push_back(pos);
                    positions.push(None);
                }
                Skip::Skip => positions.push(Some(Skip::Skip)),
            }
        }

        self.pending.insert(batch.id, PendingBatch {
            id: batch.id,
            positions,
        });

        self.maybe_finished(api, batch.id);
    }

    fn respond(&mut self, api: &mut ApiStub, mut pull: Pull) -> Result<(), Pull> {
        // Handle response.
        match pull.response.take() {
            Some(Ok(res)) => {
                let batch_id = res.batch_id;
                if let Some(pending) = self.pending.get_mut(&batch_id) {
                    if let Some(pos) = pending.positions.get_mut(res.position_id.0) {
                        info!("Finished {} {:?}", res.batch_id, res.position_id);
                        *pos = Some(Skip::Present(res));
                    }
                }
                self.maybe_finished(api, batch_id);
            }
            Some(Err(failed)) => {
                self.pending.remove(&failed.batch_id);
                self.incoming.retain(|p| p.batch_id != failed.batch_id);
            }
            None => (),
        }

        // Try to satisfy pull.
        if let Some(position) = self.incoming.pop_front() {
            if let Err(err) = pull.callback.send(position) {
                self.incoming.push_front(err);
            }
            Ok(())
        } else {
            Err(pull)
        }
    }

    fn maybe_finished(&mut self, api: &mut ApiStub, batch: BatchId) {
        if let Some(pending) = self.pending.remove(&batch) {
            match pending.try_into_completed() {
                Ok(completed) => {
                    api.submit_analysis(completed.id, completed.into_analysis());
                }
                Err(pending) => {
                    self.pending.insert(pending.id, pending);
                }
            }
        }
    }
}

#[derive(Debug)]
enum QueueMessage {
    Pull {
        callback: oneshot::Sender<Position>,
    }
}

pub struct QueueActor {
    rx: mpsc::UnboundedReceiver<QueueMessage>,
    interrupt: Arc<Notify>,
    state: Arc<Mutex<QueueState>>,
    api: ApiStub,
    opt: BacklogOpt,
    backoff: RandomizedBackoff,
}

impl QueueActor {
    fn new(rx: mpsc::UnboundedReceiver<QueueMessage>, interrupt: Arc<Notify>, state: Arc<Mutex<QueueState>>, opt: BacklogOpt, api: ApiStub) -> QueueActor {
        QueueActor {
            rx,
            interrupt,
            state,
            api,
            opt,
            backoff: RandomizedBackoff::default(),
        }
    }

    pub async fn run(self) {
        debug!("Queue actor started.");
        self.run_inner().await;
    }

    pub async fn backlog_wait_time(&mut self) -> (Duration, AcquireQuery) {
        let sec = Duration::from_secs(1);
        let performance_based_backoff = Duration::default(); // TODO
        let user_backlog = self.opt.user.map_or(Duration::default(), Duration::from) + performance_based_backoff;
        let system_backlog = self.opt.system.map_or(Duration::default(), Duration::from);

        if user_backlog >= sec || system_backlog >= sec {
            if let Some(status) = self.api.status().await {
                let user_wait = user_backlog.checked_sub(status.user.oldest).unwrap_or(Duration::default());
                let system_wait = system_backlog.checked_sub(status.system.oldest).unwrap_or(Duration::default());
                debug!("User wait: {:?} due to {:?} for oldest {:?}, system wait: {:?} due to {:?} for oldest {:?}",
                       user_wait, user_backlog, status.user.oldest,
                       system_wait, system_backlog, status.system.oldest);
                let slow = user_wait >= system_wait + sec;
                return (min(user_wait, system_wait), AcquireQuery { slow });
            }
        }

        let slow = performance_based_backoff >= sec;
        (Duration::default(), AcquireQuery { slow })
    }

    async fn run_inner(mut self) {
        while let Some(msg) = self.rx.recv().await {
            match msg {
                QueueMessage::Pull { mut callback } => {
                    loop {
                        callback = {
                            let mut state = self.state.lock().await;

                            let done = state.respond(&mut self.api, Pull {
                                response: None, // always handled in the stub
                                callback,
                            });

                            if state.shutdown_soon {
                                break;
                            }

                            match done {
                                Ok(()) => break,
                                Err(pull) => pull.callback,
                            }
                        };

                        let (wait, query) = self.backlog_wait_time().await;

                        if wait >= Duration::from_secs(120) {
                            info!("Going idle for {:?}.", wait);
                        } else if wait >= Duration::from_secs(1) {
                            debug!("Going idle for {:?}.", wait);
                        }

                        tokio::select! {
                            _ = callback.closed() => break,
                            _ = self.interrupt.notified() => continue,
                            _ = time::sleep(wait) => (),
                        }

                        match self.api.acquire(query).await {
                            Some(Acquired::Accepted(body)) => {
                                self.backoff.reset();

                                let mut state = self.state.lock().await;
                                state.add_incoming_batch(&mut self.api, IncomingBatch::from(body));
                            }
                            Some(Acquired::NoContent) => {
                                let backoff = self.backoff.next();
                                info!("No job received. Backing off {:?}.", backoff);
                                tokio::select! {
                                    _ = callback.closed() => break,
                                    _ = self.interrupt.notified() => (),
                                    _ = time::sleep(backoff) => (),
                                }
                            }
                            Some(Acquired::BadRequest) => {
                                warn!("Client update might be required. Stopping queue.");
                                let mut state = self.state.lock().await;
                                state.shutdown_soon = true;
                            },
                            None => (),
                        }
                    }
                }
            }
        }

    }
}

impl Drop for QueueActor {
    fn drop(&mut self) {
        debug!("Queue actor exited.");
    }
}

#[derive(Debug, Clone)]
enum Skip<T> {
    Present(T),
    Skip,
}

#[derive(Debug, Clone)]
pub struct IncomingBatch {
    id: BatchId,
    positions: Vec<Skip<Position>>,
}

impl From<AcquireResponseBody> for IncomingBatch {
    fn from(body: AcquireResponseBody) -> IncomingBatch {
        let mut batch = IncomingBatch {
            id: body.work.id,
            positions: Vec::new(),
        };

        let variant = body.variant.into();
        let nodes = body.nodes.unwrap_or(4_000_000);
        let mut moves = Vec::new();

        batch.positions.push(Skip::Present(Position {
            batch_id: body.work.id,
            position_id: PositionId(0),
            variant,
            fen: body.position.clone(),
            moves: moves.clone(),
            nodes,
            skill: None,
        }));

        for (i, m) in body.moves.into_iter().enumerate() {
            moves.push(m);
            batch.positions.push(Skip::Present(Position {
                batch_id: body.work.id,
                position_id: PositionId(1 + i),
                variant,
                fen: body.position.clone(),
                moves: moves.clone(),
                nodes,
                skill: None,
            }));
        }

        for skip in body.skip_positions.into_iter() {
            if let Some(pos) = batch.positions.get_mut(skip) {
                *pos = Skip::Skip;
            }
        }

        batch
    }
}

#[derive(Debug, Clone)]
struct PendingBatch {
    id: BatchId,
    positions: Vec<Option<Skip<PositionResponse>>>,
}

impl PendingBatch {
    fn try_into_completed(self) -> Result<CompletedBatch, PendingBatch> {
        match self.positions.clone().into_iter().collect() {
            Some(positions) => Ok(CompletedBatch {
                id: self.id,
                positions
            }),
            None => Err(self),
        }
    }
}

pub struct CompletedBatch {
    id: BatchId,
    positions: Vec<Skip<PositionResponse>>,
}

impl CompletedBatch {
    fn into_analysis(self) -> Vec<AnalysisPart> {
        self.positions.into_iter().map(|p| {
            match p {
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
            }
        }).collect()
    }
}
