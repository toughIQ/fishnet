use std::sync::{Arc, Mutex};
use std::fmt;
use std::cmp::{max, min};
use crate::configure::Verbose;

#[derive(Clone)]
pub struct Logger {
    verbose: Verbose,
    state: Arc<Mutex<LoggerState>>,
}

impl Logger {
    pub fn new(verbose: Verbose) -> Logger {
        Logger {
            verbose,
            state: Arc::new(Mutex::new(LoggerState { })),
        }
    }

    pub fn headline(&self, title: &str) {
        println!();
        println!("### {}", title);
        println!();
    }

    pub fn debug(&self, line: &str) {
        println!("D: {}", line);
    }

    pub fn progress(&self, line: &str) {
        println!("{}", line);
    }

    pub fn info(&self, line: &str) {
        println!("{}", line);
    }

    pub fn fishnet_info(&self, line: &str) {
        println!("><> {}", line);
    }

    pub fn warn(&self, line: &str) {
        println!("W: {}", line);
    }

    pub fn error(&self, line: &str) {
        println!("E: {}", line);
    }
}

struct LoggerState {
}

pub struct QueueStatusBar {
    pub pending: usize,
    pub cores: usize,
}

impl fmt::Display for QueueStatusBar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let width = 20;
        let virtual_width = max(self.cores, self.pending);
        let cores_width = self.cores * width / virtual_width;
        let pending_width = self.pending * width / virtual_width;

        f.write_str("[")?;
        f.write_str(&"=".repeat(min(pending_width, cores_width)))?;
        f.write_str(&" ".repeat(cores_width.saturating_sub(pending_width)))?;
        f.write_str("|")?;
        f.write_str(&"=".repeat(pending_width.saturating_sub(cores_width)))?;
        write!(f, "] {} cores / {} queued", self.cores, self.pending)
    }
}
