use std::cmp::min;
use std::time::Duration;
use rand::Rng;

#[derive(Debug, Default)]
pub struct RandomizedBackoff {
    duration: Duration,
    pub maximal_backoff_seconds: u64,
}

impl RandomizedBackoff {
    pub fn new(maximal_backoff_seconds: u64) -> RandomizedBackoff {
        RandomizedBackoff {
            duration: Duration::default(),
            maximal_backoff_seconds: maximal_backoff_seconds
        }
    }

    pub fn default() -> RandomizedBackoff {
        RandomizedBackoff {
            duration: Duration::default(),
            maximal_backoff_seconds: 30
        }
    }

    pub fn next(&mut self) -> Duration {
        let low = self.duration.as_millis() as u64;
        let high = min(self.maximal_backoff_seconds * 1000, (low + 500) * 2);
        self.duration = Duration::from_millis(rand::thread_rng().gen_range(low, high));
        self.duration
    }

    pub fn reset(&mut self) {
        self.duration = Duration::default();
    }
}

pub trait NevermindExt: Sized {
    fn nevermind(self, _msg: &str) {}
}

impl<T, E> NevermindExt for Result<T, E> {}
