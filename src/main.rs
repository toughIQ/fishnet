mod configure;
mod systemd;

use std::env;
use std::fs::OpenOptions;
use crate::configure::Command;
use tracing::error;

#[tokio::main]
async fn main() {
    let opt = configure::parse_and_configure();

    tracing::subscriber::set_global_default(
        tracing_subscriber::fmt()
            .finish()).expect("set gloabl tracing subsriber");

    if opt.auto_update {
        todo!("--auto-update");
    }

    match opt.command {
        Some(Command::Configure) => (),
        None | Some(Command::Run) => todo!("run"),
        Some(Command::Systemd) => todo!("systemd"),
        Some(Command::SystemdUser) => systemd::systemd_user(opt),
        Some(Command::Cpuid) => todo!("cpuid"),
    }
}
