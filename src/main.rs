mod configure;
mod assets;
mod systemd;
mod api;
mod ipc;
mod queue;
mod util;

use std::mem;
use std::sync::Arc;
use tracing::{debug, info, warn, error};
use tokio::signal;
use tokio::sync::{Barrier, mpsc, oneshot};
use crate::configure::{Opt, Command, Cores};
use crate::assets::Cpu;
use crate::ipc::{BatchId, Pull};
use crate::api::{Acquired, AcquireQuery};

#[tokio::main]
async fn main() {
    let opt = configure::parse_and_configure().await;

    if opt.auto_update {
        todo!("--auto-update");
    }

    match opt.command {
        Some(Command::Run) | None => run(opt).await,
        Some(Command::Systemd) => systemd::systemd_system(opt),
        Some(Command::SystemdUser) => systemd::systemd_user(opt),
        Some(Command::Configure) => (),
    }
}

async fn run(opt: Opt) {
    let cpu = Cpu::detect();
    info!("CPU features: {:?}", cpu);

    let cores = usize::from(opt.cores.unwrap_or(Cores::Auto));
    info!("Cores: {}", cores);

    // Install handler for SIGTERM.
    //#[cfg(unix)]
    //let mut sig_term = signal::unix::signal(signal::unix::SignalKind::terminate()).expect("install handler for sigterm");
    //#[cfg(not(unix))]
    let mut sig_term = {
        let (sig_term_tx, sig_term) = mpsc::channel::<Option<()>>(1);
        mem::forget(sig_term_tx);
        sig_term
    };

    // Install handler for SIGINT.
    //#[cfg(unix)]
    //let mut sig_int = signal::unix::signal(signal::unix::SignalKind::interrupt()).expect("install handler for sigint");
    //#[cfg(not(unix))]
    let mut sig_int = {
        let (mut sig_int_tx, sig_int) = mpsc::channel::<Option<()>>(1);
        tokio::spawn(async move {
            loop {
                match signal::ctrl_c().await {
                    Ok(_) => (),
                    Err(_) => break,
                }
                match sig_int_tx.send(Some(())).await {
                    Ok(_) => (),
                    Err(_) => break,
                }
            }
        });
        sig_int
    };

    // Shut down when each worker, the API actor, ~the queue actor~, and the
    // main loop have finished.
    let shutdown_barrier = Arc::new(Barrier::new(cores + 2));

    // Spawn API actor.
    let endpoint = opt.endpoint();
    info!("Endpoint: {}", endpoint);
    let api = {
        let shutdown_barrier = shutdown_barrier.clone();
        let (api, api_actor) = api::channel(endpoint, opt.key);
        tokio::spawn(async move {
            api_actor.run().await;
            shutdown_barrier.wait().await;
        });
        api
    };

    // Spawn queue actor.
    let mut queue = {
        let shutdown_barrier = shutdown_barrier.clone();
        let (queue, queue_actor) = queue::channel(api);
        tokio::spawn(async move {
            queue_actor.run().await;
            //shutdown_barrier.wait().await;
        });
        queue
    };

    // Spawn workers. Workers handle engine processes and send their results
    // to tx, thereby requesting more work.
    let mut rx = {
        let (tx, rx) = mpsc::channel::<Pull>(cores);
        for i in 0..cores {
            let mut tx = tx.clone();
            let shutdown_barrier = shutdown_barrier.clone();
            tokio::spawn(async move {
                debug!("Started worker {}.", i);

                loop {
                    tokio::time::delay_for(std::time::Duration::from_secs(5)).await;

                    let (callback, waiter) = oneshot::channel();

                    if let Err(_) = tx.send(Pull {
                        response: None,
                        callback,
                    }).await {
                        error!("Worker was about to send result, but tx is dead.");
                        break;
                    }

                    tokio::select! {
                         _ = tokio::time::delay_for(std::time::Duration::from_millis(200)) => break,
                        res = waiter => {
                            match res {
                                Ok(_) => todo!("next job"),
                                Err(_) => break,
                            }
                        }
                    }
                }

                debug!("Stopped worker {}.", i);
                drop(tx);
                shutdown_barrier.wait().await;
            });
        }
        rx
    };

    let mut shutdown_soon = false;

    // Main loop. Handles signals, forwards worker results from rx to the HTTP
    // API and responds with more work.
    loop {
        tokio::select! {
            res = sig_int.recv() => {
                res.expect("sigint handler installed");
                if shutdown_soon {
                    info!("Stopping now.");
                    rx.close(); // will not do in tokio 0.2
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
                rx.close(); // will not do in tokio 0.2
            }
            res = rx.recv() => {
                if let Some(res) = res {
                    debug!("Forwarding pull.");
                    queue.pull(res).await;
                } else {
                    debug!("All workers dropped their tx.");
                    queue.shutdown().await;
                    break;
                }
            }
        }
    }

    // Drop queue to abort remaining jobs.
    drop(queue);

    debug!("Bye.");
    shutdown_barrier.wait().await;
}
