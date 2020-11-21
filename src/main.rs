mod configure;

use structopt::StructOpt;
use std::sync::Arc;
use std::time::Duration;
use tokio::signal::unix::SignalKind;
use tokio::{signal, time, sync};

use crate::configure::Opt;

#[derive(Debug)]
enum Job {
    Analysis(AnalysisJob),
    Idle(Duration),
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
            tokio::select! {
                _ = time::sleep(Duration::from_millis(5000)) => {
                    println!("{} ... worked.", prefix);
                    Some(AnalysisResult)
                }
                _ = tx.closed() => {
                    println!("{} ... cancelled.", prefix);
                    None
                }
            }
        } else {
            None
        };

        let (next_tx, next_rx) = sync::oneshot::channel();

        if let Err(_) = tx.send(Product {
            res: job_result,
            next_tx,
        }).await {
            println!("{} no longer interested", prefix);
            break;
        }

        match next_rx.await {
            Ok(Job::Analysis(ana)) => {
                job = Some(ana);
            }
            Ok(Job::Idle(t)) => {
                println!("{} idling ...", prefix);
                tokio::select! {
                    _ = time::sleep(t) => {}
                    _ = tx.closed() => {}
                }
            }
            Err(_) => {
                println!("{} next_tx dropped", prefix);
                break;
            }
        }
    }

    time::sleep(Duration::from_millis(2000)).await;
    println!("{} shut down", prefix);
}

#[tokio::main]
async fn main() {
    let opt = configure::parse_and_configure();
    panic!("stop");

    let num_threads = 2;

    let mut ctrl_c = signal::unix::signal(SignalKind::interrupt()).expect("install signal handler");

    let (tx, mut rx) = sync::mpsc::channel(num_threads);

    let shutdown_barrier = Arc::new(sync::Barrier::new(num_threads + 1));

    for id in 1..=num_threads {
        let tx = tx.clone();
        let shutdown_barrier = shutdown_barrier.clone();
        tokio::spawn(async move {
            producer(id, tx).await;
            shutdown_barrier.wait().await;
        });
    }
    drop(tx);

    let mut in_queue: usize = 0;

    let mut shutdown_soon = false;

    loop {
        tokio::select! {
            res = ctrl_c.recv() => {
                res.expect("signal handler installed");
                println!("ctrl+c");
                if shutdown_soon {
                    println!("emergency shutdown");
                    rx.close();
                } else {
                    shutdown_soon = true;
                }
            }
            req = rx.recv() => {
                if let Some(req) = req {
                    if let Some(res) = req.res {
                        println!("got result: {:?}", res);
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
                        drop(req.next_tx);
                    } else {
                        req.next_tx.send(Job::Idle(Duration::from_millis(50))).expect("send to worker");
                    }
                } else {
                    if in_queue > 0 {
                        println!("had to abort jobs");
                    }
                    println!("rx closed and empty");
                    break;
                }
            }
        }
    }

    println!("waiting for workers to shut down ...");
    shutdown_barrier.wait().await;
    println!("... workers shut down");
}
