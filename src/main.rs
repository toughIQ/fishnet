use std::time::Duration;
use tokio::{signal, time, sync};

#[derive(Debug)]
struct Product {
    res: (),
    next_tx: sync::oneshot::Sender<()>,
}

/// Produces analysis.
async fn producer(tx: sync::mpsc::Sender<Product>) {
    loop {
        println!("working ...");
        time::sleep(Duration::from_millis(5000)).await;

        let (next_tx, next_rx) = sync::oneshot::channel();

        tx.send(Product {
            res: (),
            next_tx,
        }).await.expect("send");

        let job = next_rx.await.expect("main sender not dropped");
        dbg!(job);
    }
}

#[tokio::main]
async fn main() {
    let num_threads = 2;

    let (tx, mut rx) = sync::mpsc::channel(num_threads);

    for _ in 0..2 {
        let tx = tx.clone();
        tokio::spawn(async move {
            producer(tx).await;
        });
    }

    while let Some(req) = rx.recv().await {
        dbg!(req).next_tx.send(()).expect("send");
    }
}
