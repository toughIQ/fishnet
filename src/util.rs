use std::{
    cmp::{max, min},
    time::Duration,
};

use fastrand::Rng;

use crate::configure::MaxBackoff;

#[derive(Debug, Default)]
pub struct RandomizedBackoff {
    duration: Duration,
    max_backoff: MaxBackoff,
    rng: Rng,
}

impl RandomizedBackoff {
    pub fn new(max_backoff: MaxBackoff) -> RandomizedBackoff {
        RandomizedBackoff {
            duration: Duration::default(),
            max_backoff,
            rng: Rng::new(),
        }
    }

    pub fn next(&mut self) -> Duration {
        let low = 100;
        let cap = max(low, Duration::from(self.max_backoff).as_millis() as u64);
        let last = self.duration.as_millis() as u64;
        let high = 4 * max(low, last);
        let t = min(cap, self.rng.u64(low..high));
        self.duration = Duration::from_millis(t);
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

pub fn grow_with_and_get_mut<'a, T, F>(vec: &'a mut Vec<T>, index: usize, f: F) -> &'a mut T
where
    F: FnMut() -> T,
{
    if vec.len() < index + 1 {
        vec.resize_with(index + 1, f);
    }
    &mut vec[index]
}
