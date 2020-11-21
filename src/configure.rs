use structopt::StructOpt;
use std::path::PathBuf;
use std::str::FromStr;
use std::num::ParseIntError;
use std::time::Duration;
use url::Url;

/// Distributed Stockfish analysis for lichess.org.
#[derive(Debug, StructOpt)]
pub struct Opt {
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

#[derive(Debug, Default, StructOpt)]
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
        } else if s == "all" {
            Cores::All
        } else {
            Cores::Number(s.parse()?)
        })
    }
}

#[derive(Debug)]
enum Backlog {
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

#[derive(StructOpt, Debug)]
enum Command {
    /// Donate CPU time by running analysis (default).
    Run,
    /// Run interactive configuration.
    Configure,
    /// Generate a systemd service file.
    Systemd,
    /// Generate a systemd user service file.
    SystemdUser,
    /// Show debug information about OS and CPU.
    Cpuid,
}

#[derive(Debug, Default)]
struct Config {
    key: Option<String>,
    cores: Option<Cores>,
    endpoint: Option<Url>,
    user_backlog: Option<Backlog>,
    system_backlog: Option<Backlog>,

    // Legacy.
    engine_dir: bool,
    stockfish_command: bool,
    threads_per_process: bool,
    memory: bool,
    fixed_backoff: bool,
}

pub fn parse_and_configure() -> Opt {
    let opt = Opt::from_args();
    opt
}
