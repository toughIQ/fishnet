use std::sync::{Arc, Mutex};
use std::fmt;
use std::cmp::{max, min};
use crate::configure::Verbose;

#[derive(Clone)]
pub struct Logger {
    verbose: Verbose,
    stderr: bool,
    state: Arc<Mutex<LoggerState>>,
}

impl Logger {
    pub fn new(verbose: Verbose, stderr: bool) -> Logger {
        Logger {
            verbose,
            stderr,
            state: Arc::new(Mutex::new(LoggerState { })),
        }
    }

    fn println(&self, line: &str) {
        if self.stderr {
            eprintln!("{}", line);
        } else {
            println!("{}", line);
        }
    }

    pub fn headline(&self, title: &str) {
        self.println("");
        self.println(&format!("### {}", title));
        self.println("");
    }

    pub fn debug(&self, line: &str) {
        self.println(&format!("D: {}", line));
    }

    pub fn progress(&self, line: &str) {
        println!("{}", line); // TODO
    }

    pub fn info(&self, line: &str) {
        self.println(line);
    }

    pub fn fishnet_info(&self, line: &str) {
        self.println(&format!("><> {}", line));
    }

    pub fn warn(&self, line: &str) {
        self.println(&format!("W: {}", line));
    }

    pub fn error(&self, line: &str) {
        self.println(&format!("E: {}", line));
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
