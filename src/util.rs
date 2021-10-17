use std::{
    cmp::{max, min},
    time::Duration,
};

use rand::Rng;

#[derive(Debug)]
pub struct RandomizedBackoff {
    duration: Duration,
    max_backoff: Duration,
}

impl RandomizedBackoff {
    pub fn new(max_backoff: Duration) -> RandomizedBackoff {
        RandomizedBackoff {
            duration: Duration::default(),
            max_backoff,
        }
    }

    pub fn next(&mut self) -> Duration {
        let low = 100;
        let cap = max(low, self.max_backoff.as_millis() as u64);
        let last = self.duration.as_millis() as u64;
        let high = 4 * max(low, last);
        let t = min(cap, rand::thread_rng().gen_range(low..high));
        self.duration = Duration::from_millis(t);
        self.duration
    }

    pub fn reset(&mut self) {
        self.duration = Duration::default();
    }
}

impl Default for RandomizedBackoff {
    fn default() -> RandomizedBackoff {
        RandomizedBackoff::new(Duration::from_secs(30))
    }
}

pub trait NevermindExt: Sized {
    fn nevermind(self, _msg: &str) {}
}

impl<T, E> NevermindExt for Result<T, E> {}
