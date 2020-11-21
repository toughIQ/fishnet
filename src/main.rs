use std::time::Duration;
use tokio::signal::unix::SignalKind;
use tokio::{signal, time, sync};

#[derive(Debug)]
struct Product {
    res: (),
    next_tx: sync::oneshot::Sender<()>,
}

/// Produces analysis.
async fn producer(id: usize, tx: sync::mpsc::Sender<Product>) {
    loop {
        println!("{} working ...", " ".repeat(id * 15));
        time::sleep(Duration::from_millis(5000)).await;
        println!("{} ... worked.", " ".repeat(id * 15));

        let (next_tx, next_rx) = sync::oneshot::channel();

        tx.send(Product {
            res: (),
            next_tx,
        }).await.expect("send");

        let _job = next_rx.await.expect("main sender not dropped");
    }
}

#[tokio::main]
async fn main() {
    let num_threads = 2;

    let (tx, mut rx) = sync::mpsc::channel(num_threads);

    for id in 1..=5 {
        let tx = tx.clone();
        tokio::spawn(async move {
            producer(id, tx).await;
        });
    }

    let mut in_queue: usize = 0;

    let mut ctrl_c = signal::unix::signal(SignalKind::interrupt()).expect("install signal handler");

    loop {
        tokio::select! {
            req = rx.recv() => {
                if let Some(req) = req {
                    if in_queue == 0 {
                        println!("fetching ...");
                        time::sleep(Duration::from_millis(2000)).await;
                        println!("... fetched.");
                        in_queue += 7;
                    }

                    in_queue -= 1;
                    req.next_tx.send(()).expect("send");
                }
            }
            res = ctrl_c.recv() => {
                res.expect("signal handler installed");
                println!("ctrl+c");
                return;
            }
        }
    }
}
