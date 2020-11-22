mod configure;

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
        Some(Command::Configure) => (),
        None | Some(Command::Run) => todo!("run"),
        Some(Command::Systemd) => todo!("systemd"),
        Some(Command::SystemdUser) => todo!("systemd-user"),
        Some(Command::Cpuid) => todo!("cpuid"),
    }
}
