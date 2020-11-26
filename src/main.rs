mod configure;
mod assets;
mod systemd;
mod api;
mod ipc;
mod queue;
mod util;
mod stockfish;

use std::sync::Arc;
use std::time::{Duration, Instant};
use std::error::Error;
use std::thread;
use std::path::PathBuf;
use std::env;
use atty::Stream;
use tracing::{debug, warn, info, error};
use tokio::time;
use tokio::signal;
use tokio::sync::{mpsc, oneshot};
use crate::configure::{Opt, Command, Cores};
use crate::assets::{Assets, Cpu, ByEngineFlavor, EngineFlavor};
use crate::ipc::{Pull, Position};
use crate::stockfish::StockfishInit;
use crate::util::RandomizedBackoff;

#[tokio::main]
async fn main() {
    let opt = configure::parse_and_configure().await;

    if opt.auto_update {
        let current_exe = env::current_exe().expect("current exe");
        match auto_update(true) {
            Err(err) => error!("Failed to update: {}", err),
            Ok(self_update::Status::UpToDate(version)) => {
                info!("Fishnet is up to date: {}", version);
            }
            Ok(self_update::Status::Updated(version)) => {
                info!("Fishnet updated to {}.", version);
                restart_process(current_exe);
            }
        }
    }

    match opt.command {
        Some(Command::Run) | None => run(opt).await,
        Some(Command::Systemd) => systemd::systemd_system(opt),
        Some(Command::SystemdUser) => systemd::systemd_user(opt),
        Some(Command::Configure) => (),
    }
}

#[cfg(unix)]
fn restart_process(current_exe: PathBuf) {
    use std::os::unix::process::CommandExt as _;
    info!("Waiting 5s before restarting {:?} ...", current_exe);
    thread::sleep(Duration::from_secs(5));
    let err = std::process::Command::new(current_exe)
        .args(std::env::args().into_iter().skip(1))
        .exec();
    panic!("Failed to restart: {}", err);
}

#[cfg(windows)]
fn restart_process(current_exe: PathBuf) {
    info!("Waiting 5s before restarting {:?} ...", current_exe);
    todo!("Restart on Windows");
}

fn auto_update(verbose: bool) -> Result<self_update::Status, Box<dyn Error>> {
    info!("Checking for updates (--auto-update) ...");
    Ok(self_update::backends::github::Update::configure()
        .repo_owner("niklasf")
        .repo_name("fishnet")
        .bin_name("fishnet")
        .show_output(verbose)
        .show_download_progress(atty::is(Stream::Stdout) && verbose)
        .current_version(env!("CARGO_PKG_VERSION"))
        .no_confirm(true)
        .build()?
        .update()?)
}

async fn run(opt: Opt) {
    let cpu = Cpu::detect();
    info!("CPU features: {:?}", cpu);

    let cores = usize::from(opt.cores.unwrap_or(Cores::Auto));
    info!("Cores: {}", cores);

    // Install handler for SIGTERM.
    #[cfg(unix)]
    let mut sig_term = signal::unix::signal(signal::unix::SignalKind::terminate()).expect("install handler for sigterm");
    #[cfg(windows)]
    let mut sig_term = signal::windows::ctrl_break().expect("install handler for ctrl+break");

    // Install handler for SIGINT.
    #[cfg(unix)]
    let mut sig_int = signal::unix::signal(signal::unix::SignalKind::interrupt()).expect("install handler for sigint");
    #[cfg(windows)]
    let mut sig_int = signal::windows::ctrl_break().expect("install handler for ctrl+c"); // TODO: https://github.com/tokio-rs/tokio/issues/3178

    // To wait for workers and API actor before shutdown.
    let mut join_handles = Vec::new();

    // Spawn API actor.
    let endpoint = opt.endpoint();
    info!("Endpoint: {}", endpoint);
    let api = {
        let (api, api_actor) = api::channel(endpoint.clone(), opt.key);
        join_handles.push(tokio::spawn(async move {
            api_actor.run().await;
        }));
        api
    };

    // Spawn queue actor.
    let mut queue = {
        let (queue, queue_actor) = queue::channel(endpoint, opt.backlog, cores, api);
        join_handles.push(tokio::spawn(async move {
            queue_actor.run().await;
        }));
        queue
    };

    // Spawn workers. Workers handle engine processes and send their results
    // to tx, thereby requesting more work.
    let mut rx = {
        let assets = Arc::new(Assets::prepare(cpu).expect("prepared bundled stockfish"));
        let (tx, rx) = mpsc::channel::<Pull>(cores);
        for i in 0..cores {
            let assets = assets.clone();
            let tx = tx.clone();
            join_handles.push(tokio::spawn(async move {
                debug!("Started worker {}.", i);

                let mut job: Option<Position> = None;
                let mut engine = ByEngineFlavor {
                    official: None,
                    multi_variant: None,
                };
                let mut engine_backoff = RandomizedBackoff::default();

                loop {
                    let response = if let Some(job) = job.take() {
                        debug!("Worker {} running on {} {:?}", i, job.batch_id, job.position_id);

                        // Ensure engine process is ready.
                        let flavor = job.engine_flavor();
                        let (mut sf, join_handle) = if let Some((sf, join_handle)) = engine.get_mut(flavor).take() {
                            (sf, join_handle)
                        } else {
                            // Backoff before starting engine.
                            let backoff = engine_backoff.next();
                            if backoff >= Duration::from_secs(5) {
                                info!("Waiting {:?} before attempting to start engine.", backoff);
                            } else {
                                debug!("Waiting {:?} before attempting to start engine.", backoff);
                            }
                            tokio::select! {
                                _ = tx.closed() => break,
                                _ = time::sleep(engine_backoff.next()) => (),
                            }

                            // Start engine and spawn actor.
                            let (sf, sf_actor) = stockfish::channel(assets.stockfish.get(flavor).clone(), StockfishInit {
                                nnue: assets.nnue.clone(),
                            });
                            let join_handle = tokio::spawn(async move {
                                sf_actor.run().await;
                            });
                            (sf, join_handle)
                        };

                        // Analyse.
                        tokio::select! {
                            _ = tx.closed() => {
                                debug!("Worker {} shutting down engine early.", i);
                                drop(sf);
                                join_handle.await.expect("join");
                                break;
                            }
                            _ = time::sleep(Duration::from_secs(5 + job.nodes / 250_000)) => {
                                warn!("Engine timed out in worker {}.", i);
                                drop(sf);
                                join_handle.await.expect("join");
                                break;
                            }
                            res = sf.go(job) => {
                                match res {
                                    Ok(res) => {
                                        *engine.get_mut(flavor) = Some((sf, join_handle));
                                        engine_backoff.reset();
                                        Some(Ok(res))
                                    }
                                    Err(failed) => {
                                        drop(sf);
                                        debug!("Worker {} waiting for engine to shut down after error.", i);
                                        join_handle.await.expect("join");
                                        Some(Err(failed))
                                    },
                                }
                            }
                        }
                    } else {
                        None
                    };

                    let (callback, waiter) = oneshot::channel();

                    if let Err(_) = tx.send(Pull {
                        response,
                        callback,
                    }).await {
                        debug!("Worker {} was about to send result, but shutting down.", i);
                        break;
                    }

                    tokio::select! {
                        _ = tx.closed() => break,
                        res = waiter => {
                            match res {
                                Ok(next_job) => job = Some(next_job),
                                Err(_) => break,
                            }
                        }
                    }
                }

                if let Some((sf, join_handle)) = engine.get_mut(EngineFlavor::Official).take() {
                    debug!("Worker {} waiting for standard engine to shut down.", i);
                    drop(sf);
                    join_handle.await.expect("join");
                }

                if let Some((sf, join_handle)) = engine.get_mut(EngineFlavor::MultiVariant).take() {
                    debug!("Worker {} waiting for multi-variant engine to shut down.", i);
                    drop(sf);
                    join_handle.await.expect("join");
                }

                debug!("Stopped worker {}.", i);
                drop(tx);
            }));
        }
        rx
    };

    let restart = Arc::new(std::sync::Mutex::new(None));
    let mut up_to_date = Instant::now();
    let mut shutdown_soon = false;

    loop {
        // Check for updates from time to time.
        if opt.auto_update && !shutdown_soon && Instant::now().duration_since(up_to_date) >= Duration::from_secs(60 * 60 * 12) {
            up_to_date = Instant::now();
            let inner_restart = restart.clone();
            tokio::task::spawn_blocking(move || {
                let current_exe = env::current_exe().expect("current exe");
                match auto_update(false) {
                    Err(err) => error!("Failed to update in the background: {}", err),
                    Ok(self_update::Status::UpToDate(version)) => {
                        info!("Fishnet {} is up to date.", version);
                    }
                    Ok(self_update::Status::Updated(version)) => {
                        info!("Fishnet updated to {}. Will restart soon.", version);
                        *inner_restart.lock().expect("restart mutex") = Some(current_exe);
                    }
                }
            }).await.expect("spawn blocking update");

            if restart.lock().expect("restart mutex").is_some() {
                shutdown_soon = true;
                queue.shutdown_soon().await;
            }
        }

        // Main loop. Handles signals, forwards worker results from rx to the
        // queue and responds with more work.
        tokio::select! {
            res = sig_int.recv() => {
                res.expect("sigint handler installed");
                if shutdown_soon {
                    info!("Stopping now.");
                    rx.close();
                } else {
                    info!("Stopping soon. Press ^C again to abort pending jobs ...");
                    queue.shutdown_soon().await;
                    shutdown_soon = true;
                }
            }
            res = sig_term.recv() => {
                res.expect("sigterm handler installed");
                info!("Stopping now.");
                shutdown_soon = true;
                rx.close();
            }
            res = rx.recv() => {
                if let Some(res) = res {
                    queue.pull(res).await;
                } else {
                    debug!("About to exit.");
                    break;
                }
            }
        }
    }

    // Shutdown queue to abort remaining jobs.
    queue.shutdown().await;

    // Wait for all workers.
    info!("Bye.");
    for join_handle in join_handles.into_iter() {
        join_handle.await.expect("join");
    }

    // Restart.
    let mut restart = restart.lock().expect("restart mutex");
    if let Some(restart) = restart.take() {
        restart_process(restart);
    }
}
