use std::time::Duration;
use tokio::signal::unix::SignalKind;
use tokio::{signal, time, sync, process};

#[derive(Debug)]
enum Job {
    Analysis(AnalysisJob),
    Idle,
    Shutdown,
}

#[derive(Debug)]
struct AnalysisJob;

#[derive(Debug)]
struct AnalysisResult;

#[derive(Debug)]
struct Product {
    res: Option<AnalysisResult>,
    next_tx: sync::oneshot::Sender<Job>,
}

/// Produces analysis.
async fn producer(id: usize, tx: sync::mpsc::Sender<Product>) {
    //let mut child = process::Command::new("stockfish")
    //    .spawn()
    //    .expect("start stockfish");

    let mut job: Option<AnalysisJob> = None;

    let prefix = " ".repeat(id * 15);

    loop {
        let job_result = if let Some(job) = job.take() {
            println!("{} working ({:?}) ...", prefix, job);
            time::sleep(Duration::from_millis(5000)).await;
            println!("{} ... worked.", prefix);
            Some(AnalysisResult)
        } else {
            None
        };

        let (next_tx, next_rx) = sync::oneshot::channel();

        tx.send(Product {
            res: job_result,
            next_tx,
        }).await.expect("send");

        match next_rx.await {
            Ok(Job::Analysis(ana)) => {
                job = Some(ana);
            }
            Ok(Job::Idle) => {
                println!("{} idling ...", prefix);
            }
            Ok(Job::Shutdown) => {
                println!("{} shutting down", prefix);
                break;
            }
            Err(_) => {
                println!("{} next_tx dropped", prefix);
            }
        }
    }

    time::sleep(Duration::from_millis(2000)).await;
    println!("{} shut down", prefix);
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
    drop(tx);

    let mut in_queue: usize = 0;

    let mut ctrl_c = signal::unix::signal(SignalKind::interrupt()).expect("install signal handler");

    let mut shutdown_soon = false;

    loop {
        tokio::select! {
            req = rx.recv() => {
                if let Some(req) = req {
                    if let Some(res) = req.res {
                        println!("got result");
                    }

                    if in_queue == 0 {
                        if shutdown_soon {
                            println!("fetching no more!");
                        } else {
                            println!("fetching ...");
                            time::sleep(Duration::from_millis(2000)).await;
                            println!("... fetched.");
                            in_queue += 7;
                        }
                    }

                    if in_queue > 0 {
                        in_queue -= 1;
                        req.next_tx.send(Job::Analysis(AnalysisJob)).expect("send to worker");
                    } else if shutdown_soon {
                        req.next_tx.send(Job::Shutdown).expect("send to worker");
                    } else {
                        req.next_tx.send(Job::Idle).expect("send to worker");
                    }
                } else {
                    println!("all workers exited");
                    return;
                }
            }
            res = ctrl_c.recv() => {
                res.expect("signal handler installed");
                println!("ctrl+c");
                shutdown_soon = true;
            }
        }
    }
}
