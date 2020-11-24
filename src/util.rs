use std::cmp::min;
use std::time::Duration;
use rand::Rng;

#[derive(Default)]
pub struct RandomizedBackoff {
    duration: Duration,
}

impl RandomizedBackoff {
    pub fn next(&mut self) -> Duration {
        let low = self.duration.as_millis() as u64;
        let high = min(60_000, (low + 500) * 2);
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
