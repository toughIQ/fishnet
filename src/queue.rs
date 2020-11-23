use std::collections::{VecDeque, HashMap};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::time;
use tracing::debug;
use crate::api::ApiStub;
use crate::ipc::{BatchId, Position, PositionResponse, Pull};
use crate::util::WhateverExt as _ ;

pub fn channel(api: ApiStub) -> (QueueStub, QueueActor) {
    let state = Arc::new(Mutex::new(QueueState::default()));
    let (tx, rx) = mpsc::unbounded_channel();
    (QueueStub::new(tx, state.clone(), api.clone()), QueueActor::new(rx, state, api))
}

#[derive(Clone)]
pub struct QueueStub {
    tx: mpsc::UnboundedSender<QueueMessage>,
    state: Arc<Mutex<QueueState>>,
    api: ApiStub,
}

impl QueueStub {
    fn new(tx: mpsc::UnboundedSender<QueueMessage>, state: Arc<Mutex<QueueState>>, api: ApiStub) -> QueueStub {
        QueueStub { tx, state, api }
    }

    pub async fn pull(&mut self, pull: Pull) {
        let position = {
            let mut state = self.state.lock().await;

            if let Some(response) = pull.response {
                state.add_position_response(&mut self.api, response);
            }

            state.incoming.pop_front()
        };

        if let Some(position) = position {
            pull.callback.send(position);
        } else {
            self.tx.send(QueueMessage::Pull {
                callback: pull.callback,
            }).whatever("queue dropped");
        }
    }

    pub async fn shutdown_soon(&mut self) {
        let mut state = self.state.lock().await;
        state.shutdown_soon = true;
    }

    pub async fn shutdown(&mut self) {
        let mut state = self.state.lock().await;
        state.shutdown_soon = true;
        state.incoming.clear();
        for (k, _) in state.pending.drain() {
            self.api.abort(k);
        }
    }
}

#[derive(Default)]
struct QueueState {
    shutdown_soon: bool,
    incoming: VecDeque<Position>,
    pending: HashMap<BatchId, PendingBatch>,
}

impl QueueState {
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

    fn add_position_response(&mut self, api: &mut ApiStub, res: PositionResponse) {
        let batch_id = res.batch_id;
        if let Some(pending) = self.pending.get_mut(&batch_id) {
            if let Some(pos) = pending.positions.get_mut(res.position_id.0) {
                *pos = Some(Skip::Present(res));
            }
        }

        self.maybe_finished(api, batch_id);
    }

    fn maybe_finished(&mut self, api: &mut ApiStub, batch: BatchId) {
        if let Some(pending) = self.pending.remove(&batch) {
            match pending.try_into_completed() {
                Ok(completed) => todo!("submit to api"),
                Err(pending) => {
                    self.pending.insert(pending.id, pending);
                },
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
    api: ApiStub,
    state: Arc<Mutex<QueueState>>,
}

impl QueueActor {
    fn new(rx: mpsc::UnboundedReceiver<QueueMessage>, state: Arc<Mutex<QueueState>>, api: ApiStub) -> QueueActor {
        QueueActor {
            rx,
            state,
            api,
        }
    }

    pub async fn run(mut self) {
        debug!("Queue actor started.");
        self.run_inner().await;
        debug!("Queue actor exited.");
    }

    async fn run_inner(mut self) {
        while let Some(msg) = self.rx.recv().await {
            match msg {
                QueueMessage::Pull { mut callback } => {
                    loop {
                        {
                            let state = self.state.lock().await;
                            if state.shutdown_soon {
                                return;
                            }
                        }

                        // TODO: Simulated failed network request.
                        time::sleep(Duration::from_millis(2000)).await;

                        // Simulated backoff.
                        tokio::select! {
                            _ = callback.closed() => break,
                            _ = time::sleep(Duration::from_millis(10_000)) => (),
                        }
                    }
                }
            }
        }

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
