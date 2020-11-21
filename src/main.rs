use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::signal::unix::SignalKind;
use tokio::{signal, time, sync};
use structopt::StructOpt;
use std::path::PathBuf;
use std::num::ParseIntError;
use url::Url;

#[derive(Debug, StructOpt)]
struct Opt {
    /// Increase verbosity.
    #[structopt(flatten)]
    verbose: Verbose,

    /// Automatically install available updates on startup and at random
    /// intervals.
    #[structopt(long, global = true)]
    auto_update: bool,

    /// Do not use a configuration file.
    #[structopt(long, conflicts_with = "conf", global = true)]
    no_conf: bool,

    /// Configuration file.
    #[structopt(long, parse(from_os_str), global = true)]
    conf: Option<PathBuf>,

    /// Fishnet API key.
    #[structopt(long, alias = "apikey", short = "k", global = true)]
    key: Option<String>,

    /// Lichess HTTP endpoint.
    #[structopt(long, global = true)]
    endpoint: Option<Url>,

    /// Number of logical CPU cores to use for engine processes
    /// (or auto for n - 1, or all for n).
    #[structopt(long, alias = "threads", global = true)]
    cores: Option<Cores>,

    /// Prefer to run high-priority jobs only if older than this duration
    /// (for example 120s).
    #[structopt(long, global = true)]
    user_backlog: Option<Backlog>,

    /// Prefer to run low-priority jobs only if older than this duration
    /// (for example 2h).
    #[structopt(long, global = true)]
    system_backlog: Option<Backlog>,

    #[structopt(subcommand)]
    command: Option<Command>,

    #[structopt(flatten)]
    legacy: Legacy,
}

#[derive(Debug, StructOpt)]
struct Verbose {
    #[structopt(name = "verbose", short = "v", parse(from_occurrences), global = true)]
    level: u32,
}

#[derive(Debug, StructOpt)]
struct Legacy {
    #[structopt(long, global = true, hidden = true)]
    memory: Option<String>,

    #[structopt(long, parse(from_os_str), global = true, hidden = true)]
    engine_dir: Option<PathBuf>,

    #[structopt(long, global = true, hidden = true)]
    stockfish_command: Option<String>,

    #[structopt(long, global = true, hidden = true)]
    threads_per_process: Option<u32>,

    #[structopt(long, global = true, hidden = true)]
    fixed_backoff: bool,

    #[structopt(long, conflicts_with = "fixed-backoff", global = true, hidden = true)]
    no_fixed_backoff: bool,

    #[structopt(long, short = "o", number_of_values = 2, multiple = true, global = true, hidden = true)]
    setoption: Vec<String>,
}

#[derive(Debug)]
enum Cores {
    Auto,
    All,
    Number(u32),
}

impl FromStr for Cores {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "auto" {
            Ok(Cores::Auto)
        } else if s == "all" {
            Ok(Cores::All)
        } else {
            Ok(Cores::Number(s.parse()?))
        }
    }
}

#[derive(Debug)]
enum Backlog {
    Short,
    Long,
    Duration(Duration),
}

impl FromStr for Backlog {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "short" {
            Ok(Backlog::Short)
        } else if s == "long" {
            Ok(Backlog::Long)
        } else {
            todo!("parse duration")
        }
    }
}

#[derive(StructOpt, Debug)]
enum Command {
    /// Run analysis (default).
    Run,
    /// Run interactive configuration.
    Configure,
    /// Generate a systemd service file.
    Systemd,
    /// Generate a systemd user service file.
    SystemdUser,
    /// Show debug information for OS and CPU.
    Cpuid,
}

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
    let opt = Opt::from_args();
    dbg!(opt);

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
