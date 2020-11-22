use atty::Stream;
use crate::configure::Opt;

pub fn systemd_user(opt: Opt) {
    let exe = std::env::current_exe().expect("current exe");
    println!("[Unit]");
    println!("Description=Fishnet client");
    println!("After=network-online.target");
    println!("Wants=network-online.target");
    println!();
    println!("[Service]");
    println!("ExecStart={:?}", exe);
    println!("KillMode=mixed");
    println!("WorkingDirectory=/tmp");
    println!("Nice=5");
    println!("PrivateTmp=true");
    println!("DevicePolicy=closed");
    if opt.auto_update && exe.starts_with("/usr/") {
        println!("ProtectSystem=false");
    } else {
        println!("ProtectSystem=full");
    }
    println!("Restart=always");
    println!();
    println!("[Install]");
    println!("WantedBy=default.target");

    if atty::is(Stream::Stdout) {
        eprintln!();
        eprintln!("# Example usage:");
        eprintln!("# {} systemd-user | tee ~/.config/systemd/user/fishnet.service", std::env::args().next().unwrap_or("./fishnet".to_owned()));
        eprintln!("# systemctl enable --user fishnet.service");
        eprintln!("# systemctl start --user fishnet.service");
        eprintln!("# Live view of log: journalctl --follow --user-unit fishnet");
    }
}
