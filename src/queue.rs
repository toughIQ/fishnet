use std::collections::{VecDeque, HashMap};
use tokio::sync::{mpsc, oneshot};
use crate::api::ApiStub;
use crate::ipc::{BatchId, Position, PositionResponse};

pub fn channel(api: ApiStub) -> (QueueStub, QueueActor) {
    let (tx, rx) = mpsc::unbounded_channel();
    (QueueStub::new(tx), QueueActor::new(rx, api))
}

#[derive(Clone)]
pub struct QueueStub {
    tx: mpsc::UnboundedSender<QueueMessage>,
}

impl QueueStub {
    fn new(tx: mpsc::UnboundedSender<QueueMessage>) -> QueueStub {
        QueueStub { tx }
    }

    pub fn pull(&mut self, callback: oneshot::Sender<Position>) {
        self.tx.send(QueueMessage::Pull { callback });
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
}

impl QueueActor {
    fn new(rx: mpsc::UnboundedReceiver<QueueMessage>, api: ApiStub) -> QueueActor {
        QueueActor {
            rx,
            api,
        }
    }

    pub async fn run(mut self) {
        while let Some(msg) = self.rx.recv().await {
            match msg {
                QueueMessage::Pull { callback } => todo!("impl pull"),
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

pub struct Queue {
    incoming: VecDeque<Position>,
    pending: HashMap<BatchId, PendingBatch>,
    completed: VecDeque<CompletedBatch>,
}

impl Queue {
    pub fn add_incoming_batch(&mut self, batch: IncomingBatch) {
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

        self.maybe_finished(batch.id);
    }

    pub fn add_position_response(&mut self, res: PositionResponse) {
        let batch_id = res.batch_id;
        if let Some(pending) = self.pending.get_mut(&batch_id) {
            if let Some(pos) = pending.positions.get_mut(res.position_id.0) {
                *pos = Some(Skip::Present(res));
            }
        }

        self.maybe_finished(batch_id);
    }

    pub fn take_incoming(&mut self) -> Option<Position> {
        self.incoming.pop_front()
    }

    pub fn take_completed(&mut self) -> Option<CompletedBatch> {
        self.completed.pop_front()
    }

    pub fn is_empty(&self) -> bool {
        self.incoming.is_empty()
    }

    pub fn abort_all(&mut self) -> Vec<BatchId> {
        self.incoming.clear();
        self.pending.drain().map(|(k, v)| k).collect()
    }

    fn maybe_finished(&mut self, batch: BatchId) {
        if let Some(pending) = self.pending.remove(&batch) {
            match pending.try_into_completed() {
                Ok(completed) => self.completed.push_back(completed),
                Err(pending) => {
                    self.pending.insert(pending.id, pending);
                },
            }
        }
    }
}
