mod configure;
mod run;
mod systemd;

use crate::configure::Command;

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
        Some(Command::Run) | None => run::run(opt),
        Some(Command::Systemd) => systemd::systemd_system(opt),
        Some(Command::SystemdUser) => systemd::systemd_user(opt),
        Some(Command::Configure) => (),
    }
}
