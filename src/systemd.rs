use std::env;
use std::fs;
use atty::Stream;
use shell_escape::escape;
use crate::configure::{Opt, Key};

pub fn systemd_system(opt: Opt) {
    let exe = exec_start(&opt);
    println!("[Unit]");
    println!("Description=Fishnet client");
    println!("After=network-online.target");
    println!("Wants=network-online.target");
    println!();
    println!("[Service]");
    println!("ExecStart={}", exe);
    println!("KillMode=mixed");
    println!("WorkingDirectory=/tmp");
    println!("User={}", env::var("USER").unwrap_or("XXX".to_owned()));
    println!("Nice=5");
    println!("CapabilityBoundingSet=");
    println!("PrivateTmp=true");
    println!("PrivateDevices=true");
    println!("DevicePolicy=closed");
    if opt.auto_update && exe.starts_with("/usr/") {
        println!("ProtectSystem=false");
    } else {
        println!("ProtectSystem=full");
    }
    println!("NoNewPrivileges=true");
    println!("Restart=on-failure");
    println!();
    println!("[Install]");
    println!("WantedBy=multi-user.target");

    if atty::is(Stream::Stdout) {
        let command = env::args().next().unwrap_or("./fishnet".to_owned());
        eprintln!();
        eprintln!("# Example usage:");
        eprintln!("# {} systemd | tee /etc/systemd/system/fishnet.service", command);
        eprintln!("# systemctl enable fishnet.service");
        eprintln!("# systemctl start fishnet.service");
        eprintln!("# Live view of log: journalctl --follow -u fishnet");
        eprintln!("# Need a user unit? {} systemd-user", command);
    }
}

pub fn systemd_user(opt: Opt) {
    let exe = exec_start(&opt);
    println!("[Unit]");
    println!("Description=Fishnet client");
    println!("After=network-online.target");
    println!("Wants=network-online.target");
    println!();
    println!("[Service]");
    println!("ExecStart={}", exe);
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
    println!("Restart=on-failure");
    println!();
    println!("[Install]");
    println!("WantedBy=default.target");

    if atty::is(Stream::Stdout) {
        eprintln!();
        eprintln!("# Example usage:");
        eprintln!("# {} systemd-user | tee ~/.config/systemd/user/fishnet.service", env::args().next().unwrap_or("./fishnet".to_owned()));
        eprintln!("# systemctl enable --user fishnet.service");
        eprintln!("# systemctl start --user fishnet.service");
        eprintln!("# Live view of log: journalctl --follow --user-unit fishnet");
    }
}

fn exec_start(opt: &Opt) -> String {
    let exe = env::current_exe().expect("current exe").to_str().expect("printable exec path").to_owned();
    let mut builder = vec![escape(exe.into()).into_owned()];
    if opt.verbose.level > 0 {
        builder.push(format!("-{}", "v".repeat(opt.verbose.level)));
    }
    if opt.auto_update {
        builder.push("--auto-update".to_owned());
    }
    if opt.no_conf {
        builder.push("--no-conf".to_owned());
    } else {
        builder.push("--conf".to_owned());
        let canonical = fs::canonicalize(&opt.conf)
            .expect("canonicalize config path")
            .to_str()
            .expect("printable config path").to_owned();
        builder.push(escape(canonical.into()).into_owned());
    }
    if let Some(Key(ref key)) = opt.key {
        builder.push("--key".to_owned());
        builder.push(escape(key.into()).into_owned());
    }
    if let Some(ref endpoint) = opt.endpoint {
        builder.push("--endpoint".to_owned());
        builder.push(escape(endpoint.to_string().into()).into_owned());
    }
    if let Some(ref cores) = opt.cores {
        builder.push("--cores".to_owned());
        builder.push(escape(cores.to_string().into()).into_owned());
    }
    if let Some(ref user_backlog) = opt.backlog.user {
        builder.push("--user-backlog".to_owned());
        builder.push(escape(user_backlog.to_string().into()).into_owned());
    }
    if let Some(ref system_backlog) = opt.backlog.system {
        builder.push("--system_backlog".to_owned());
        builder.push(escape(system_backlog.to_string().into()).into_owned());
    }
    builder.push("run".to_owned());
    builder.join(" ")
}
