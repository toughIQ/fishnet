use std::fmt;
use std::cmp::{max, min};

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
