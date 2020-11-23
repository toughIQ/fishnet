use structopt::StructOpt;
use std::fs;
use std::io;
use std::cmp::max;
use std::fmt;
use std::io::Write;
use std::path::PathBuf;
use std::str::FromStr;
use std::num::{ParseIntError, NonZeroUsize};
use std::time::Duration;
use url::Url;
use configparser::ini::Ini;
use tracing::warn;
use crate::api;

const DEFAULT_ENDPOINT: &str = "https://lichess.org/fishnet";

/// Distributed Stockfish analysis for lichess.org.
#[derive(Debug, StructOpt)]
#[structopt(setting = structopt::clap::AppSettings::DisableHelpSubcommand)]
pub struct Opt {
    #[structopt(flatten)]
    pub verbose: Verbose,

    /// Automatically install available updates on startup and at random
    /// intervals.
    #[structopt(long, global = true)]
    pub auto_update: bool,

    /// Configuration file.
    #[structopt(long, parse(from_os_str), default_value = "fishnet.ini", global = true)]
    pub conf: PathBuf,

    /// Do not use a configuration file.
    #[structopt(long, conflicts_with = "conf", global = true)]
    pub no_conf: bool,

    /// Fishnet API key.
    #[structopt(long, alias = "apikey", short = "k", global = true)]
    pub key: Option<Key>,

    /// Lichess HTTP endpoint.
    #[structopt(long, global = true)]
    pub endpoint: Option<Url>,

    /// Number of logical CPU cores to use for engine processes
    /// (or auto for n - 1, or all for n).
    #[structopt(long, alias = "threads", global = true)]
    pub cores: Option<Cores>,

    /// Prefer to run high-priority jobs only if older than this duration
    /// (for example 120s).
    #[structopt(long, global = true)]
    pub user_backlog: Option<Backlog>,

    /// Prefer to run low-priority jobs only if older than this duration
    /// (for example 2h).
    #[structopt(long, global = true)]
    pub system_backlog: Option<Backlog>,

    #[structopt(subcommand)]
    pub command: Option<Command>,
}

impl Opt {
    pub fn endpoint(&self) -> Url {
        if let Some(ref endpoint) = self.endpoint {
            endpoint.clone()
        } else {
            DEFAULT_ENDPOINT.parse().expect("default endpoint is valid")
        }
    }
}

#[derive(Debug, Default, StructOpt)]
pub struct Verbose {
    /// Increase verbosity.
    #[structopt(long = "verbose", short = "v", parse(from_occurrences), global = true)]
    pub level: usize,
}

#[derive(Debug)]
pub struct Key(pub String);

#[derive(Debug)]
pub enum KeyError {
    EmptyKey,
    InvalidKey,
    AccessDenied,
}

impl fmt::Display for KeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyError::EmptyKey => f.write_str("key expected to be non-empty"),
            KeyError::InvalidKey => f.write_str("key expected to be alphanumeric"),
            KeyError::AccessDenied => f.write_str("access denied"),
        }
    }
}

impl FromStr for Key {
    type Err = KeyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            Err(KeyError::EmptyKey)
        } else if !s.chars().all(|c| char::is_ascii_alphanumeric(&c)) {
            Err(KeyError::InvalidKey)
        } else {
            Ok(Key(s.to_owned()))
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum Cores {
    Auto,
    All,
    Number(NonZeroUsize),
}

impl Default for Cores {
    fn default() -> Cores {
        Cores::Auto
    }
}

impl FromStr for Cores {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(if s == "auto" {
            Cores::Auto
        } else if s == "all" || s == "max" {
            Cores::All
        } else {
            Cores::Number(s.parse()?)
        })
    }
}

impl fmt::Display for Cores {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Cores::Auto => f.write_str("auto"),
            Cores::All => f.write_str("all"),
            Cores::Number(n) => write!(f, "{}", n),
        }
    }
}

impl From<Cores> for usize {
    fn from(cores: Cores) -> usize {
        match cores {
            Cores::Number(n) => usize::from(n),
            Cores::Auto => max(1, num_cpus::get() - 1),
            Cores::All => num_cpus::get(),
        }
    }
}

#[derive(Debug)]
pub enum Backlog {
    Short,
    Long,
    Duration(Duration),
}

impl Default for Backlog {
    fn default() -> Backlog {
        Backlog::Duration(Duration::default())
    }
}

impl FromStr for Backlog {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(if s == "short" {
            Backlog::Short
        } else if s == "long" {
            Backlog::Long
        } else {
            let (s, factor) = if let Some(s) = s.strip_suffix("d") {
                (s, 60 * 60 * 24)
            } else if let Some(s) = s.strip_suffix("h") {
                (s, 60 * 60)
            } else if let Some(s) = s.strip_suffix("m") {
                (s, 60)
            } else {
                (s.strip_suffix("s").unwrap_or(s), 1)
            };
            Backlog::Duration(Duration::from_secs(u64::from(s.trim().parse::<u32>()?) * factor))
        })
    }
}

impl fmt::Display for Backlog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Backlog::Short => f.write_str("short"),
            Backlog::Long => f.write_str("long"),
            Backlog::Duration(d) => write!(f, "{}s", d.as_secs()),
        }
    }
}

#[derive(StructOpt, Debug, PartialEq, Eq)]
pub enum Command {
    /// Donate CPU time by running analysis (default).
    Run,
    /// Run interactive configuration.
    Configure,
    /// Generate a systemd service file.
    Systemd,
    /// Generate a systemd user service file.
    SystemdUser,
}

#[derive(Debug)]
enum Toggle {
    Yes,
    No,
    Default,
}

impl Default for Toggle {
    fn default() -> Toggle {
        Toggle::Default
    }
}

impl FromStr for Toggle {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim().to_lowercase();
        match s.as_str() {
            "y" | "j" | "yes" | "yep" | "yay" | "true" | "t" | "1" | "ok" => Ok(Toggle::Yes),
            "n" | "no" | "nop" | "nope" | "nay" | "f" | "false" | "0" => Ok(Toggle::No),
            "" => Ok(Toggle::Default),
            _ => Err(()),
        }
    }
}

fn intro() {
    println!(r#"#   _________         .    ."#);
    println!(r#"#  (..       \_    ,  |\  /|"#);
    println!(r#"#   \       O  \  /|  \ \/ /"#);
    println!(r#"#    \______    \/ |   \  /      _____ _     _     _   _      _"#);
    println!(r#"#       vvvv\    \ |   /  |     |  ___(_)___| |__ | \ | | ___| |_"#);
    println!(r#"#       \^^^^  ==   \_/   |     | |_  | / __| '_ \|  \| |/ _ \ __|"#);
    println!(r#"#        `\_   ===    \.  |     |  _| | \__ \ | | | |\  |  __/ |_"#);
    println!(r#"#        / /\_   \ /      |     |_|   |_|___/_| |_|_| \_|\___|\__| {}"#, env!("CARGO_PKG_VERSION"));
    println!(r#"#        |/   \_  \|      /"#);
    println!(r#"#               \________/      Distributed Stockfish analysis for lichess.org"#);
    println!();
}

pub async fn parse_and_configure() -> Opt {
    let mut opt = Opt::from_args();

    // Show intro and configure logger.
    (match opt.command {
        Some(Command::Systemd) | Some(Command::SystemdUser) => {
            tracing::subscriber::set_global_default(
                tracing_subscriber::fmt().with_writer(io::stderr).finish())
        },
        _ => {
            intro();
            tracing::subscriber::set_global_default(tracing_subscriber::fmt().finish())
        }
    }).expect("set global tracing subsriber");

    // Handle config file.
    if !opt.no_conf || opt.command == Some(Command::Configure) {
        let mut ini = Ini::new();
        ini.set_default_section("Fishnet");

        // Load ini.
        let file_found = match fs::read_to_string(&opt.conf) {
            Ok(contents) => {
                ini.read(contents).expect("parse config file");
                true
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => false,
            Err(err) => panic!("failed to open config file: {}", err),
        };

        // Configuration dialog.
        if (!file_found && opt.command != Some(Command::Run)) || opt.command == Some(Command::Configure) {
            eprintln!("### Configuration");

            // Step 1: Endpoint.
            eprintln!();
            let endpoint = loop {
                let mut endpoint = String::new();
                eprint!("Endpoint (default: {}): ", ini.get("Fishnet", "Endpoint").unwrap_or(DEFAULT_ENDPOINT.to_owned()));
                io::stderr().flush().expect("flush stderr");
                io::stdin().read_line(&mut endpoint).expect("read endpoint from stdin");

                let endpoint = Some(endpoint.trim().to_owned())
                    .filter(|e| !e.is_empty())
                    .or_else(|| ini.get("Fishnet", "Endpoint"))
                    .unwrap_or(DEFAULT_ENDPOINT.to_owned());

                match Url::from_str(&endpoint) {
                    Ok(url) => {
                        ini.setstr("Fishnet", "Endpoint", Some(&endpoint));
                        break opt.endpoint.clone().unwrap_or(url);
                    }
                    Err(err) => eprintln!("Invalid: {}", err),
                }
            };

            // Step 2: Key.
            let mut api = api::spawn(endpoint.clone());
            eprintln!();
            loop {
                let mut key = String::new();
                let required = if let Some(current) = ini.get("Fishnet", "Key") {
                    eprint!("Personal fishnet key (append ! to force, default: keep {}): ", "*".repeat(current.chars().count()));
                    false
                } else if endpoint.host_str() == Some("lichess.org") {
                    eprint!("Personal fishnet key (append ! to force, https://lichess.org/get-fishnet): ");
                    true
                } else {
                    eprint!("Personal fishnet key (append ! to force, probably not required): ");
                    false
                };

                io::stderr().flush().expect("flush stderr");
                io::stdin().read_line(&mut key).expect("read key from stdin");

                let key = key.trim();
                let (key, network) = if key.is_empty() {
                    if required {
                        eprintln!("Key required.");
                        continue;
                    } else {
                        break;
                    }
                } else if let Some(key) = key.strip_suffix("!") {
                    (key, false)
                } else {
                    (key, true)
                };

                let key = match Key::from_str(key) {
                    Ok(key) if network => match api.check_key(key).await {
                        Some(res) => res,
                        None => continue, // server/network error already logged
                    },
                    Ok(key) => Ok(key),
                    Err(err) => Err(err),
                };

                match key  {
                    Ok(Key(key)) => {
                        ini.set("Fishnet", "Key", Some(key));
                        break;
                    }
                    Err(err) => eprintln!("Invalid: {}", err),
                }
            }

            // Step 3: Cores.
            eprintln!();
            loop {
                let mut cores = String::new();
                let all = num_cpus::get();
                let auto = max(all - 1, 1);
                eprint!("Number of logical cores to use for engine threads (default {}, max {}): ", auto, all);
                io::stderr().flush().expect("flush stderr");
                io::stdin().read_line(&mut cores).expect("read cores from stdin");

                match Some(cores.trim()).filter(|c| !c.is_empty()).map(Cores::from_str).unwrap_or(Ok(Cores::Auto)) {
                    Ok(Cores::Number(n)) if usize::from(n) > all => {
                        eprintln!("At most {} logical cores available on your machine.", all);
                    }
                    Ok(cores) => {
                        ini.set("Fishnet", "Cores", Some(cores.to_string()));
                        break;
                    }
                    Err(err) => eprintln!("Invalid: {}", err),
                }
            }

            // Step 4: Backlog.
            eprintln!();
            eprintln!("You can choose to not join unless a backlog is building up. Examples:");
            eprintln!("* Rented server exclusively for fishnet: choose no");
            eprintln!("* Running on a laptop: choose yes");
            loop {
                let mut backlog = String::new();
                eprint!("Would you prefer to keep your client idle? (default: no) ");
                io::stderr().flush().expect("flush stderr");
                io::stdin().read_line(&mut backlog).expect("read backlog from stdin");

                match Toggle::from_str(&backlog) {
                    Ok(Toggle::Yes) => {
                        ini.setstr("Fishnet", "UserBacklog", Some("short"));
                        ini.setstr("Fishnet", "SystemBacklog", Some("long"));
                        break;
                    }
                    Ok(Toggle::No) | Ok(Toggle::Default) => {
                        ini.setstr("Fishnet", "UserBacklog", Some("0"));
                        ini.setstr("Fishnet", "SystemBacklog", Some("0"));
                        break;
                    }
                    Err(_) => (),
                }
            }

            // Step 5: Write config.
            eprintln!();
            loop {
                let mut write = String::new();
                eprint!("Done. Write configuration to {:?} now? (default: yes) ", opt.conf);
                io::stderr().flush().expect("flush stderr");
                io::stdin().read_line(&mut write).expect("read confirmation from stdin");

                match Toggle::from_str(&write) {
                    Ok(Toggle::Yes) | Ok(Toggle::Default) => {
                        let contents = ini.writes();
                        fs::write(&opt.conf, contents).expect("write config");
                        break;
                    }
                    _ => (),
                }

            }

            eprintln!();
        }

        // Merge config file into command line arguments.
        match opt.command {
            Some(Command::Systemd) | Some(Command::SystemdUser) => (),
            _ => {
                opt.endpoint = opt.endpoint.or_else(|| {
                    ini.get("Fishnet", "Endpoint").map(|e| e.parse().expect("valid endpoint"))
                });

                opt.key = opt.key.or_else(|| {
                    ini.get("Fishnet", "Key").map(|k| k.parse().expect("valid key"))
                });

                opt.cores = opt.cores.or_else(|| {
                    ini.get("Fishnet", "Cores").map(|c| c.parse().expect("valid cores"))
                });

                opt.user_backlog = opt.user_backlog.or_else(|| {
                    ini.get("Fishnet", "UserBacklog").map(|b| b.parse().expect("valid user backlog"))
                });
                opt.system_backlog = opt.system_backlog.or_else(|| {
                    ini.get("Fishnet", "SystemBacklog").map(|b| b.parse().expect("valid system backlog"))
                });
            }
        }
    }

    // Validate number of cores.
    let all = num_cpus::get();
    match opt.cores {
        Some(Cores::Number(n)) if usize::from(n) > all => {
            warn!("Requested logical {} cores, but only {} available. Capped.", n, all);
            opt.cores = Some(Cores::All);
        }
        _ => (),
    }

    opt
}
