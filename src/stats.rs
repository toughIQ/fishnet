use std::fmt;
use std::cmp::{max, min};
use std::fs::File;
use std::io::{Error, Read, Write, ErrorKind};
use std::time::Duration;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

const STATS_FILENAME: &str = ".fishnet-stats";

type OnBatchRecordedCallback = Option<fn(&StatsRecorder) -> ()>;

pub struct StatsRecorderFactory {}

impl StatsRecorderFactory {
    pub fn create_stats_recorder(cores: usize) -> StatsRecorder {
        StatsRecorderFactory::stats_path()
            .and_then(|path| File::open(path))
            .and_then(|file| StatsRecorderFactory::try_create_recorder_from_stats_file(file, cores))
            .unwrap_or_else(|_| {
                println!("Creating a new stats file ~/{}", STATS_FILENAME);
                StatsRecorder::new(cores, Some(StatsRecorderFactory::try_update_stats))
            })
    }

    fn stats_path() -> Result<PathBuf, Error> {
        match home::home_dir() {
            Some(mut path) => {
                path.push(STATS_FILENAME);
                Ok(path)
            }
            None => Err(())
        }.map_err(|_| Error::new(ErrorKind::Other, "E: Couldn't determine stats file path"))
    }

    fn try_create_recorder_from_stats_file(file: File, cores: usize) -> Result<StatsRecorder, Error> {
        match StatsRecorderFactory::create_recorder_from_stats_file(file, cores) {
            Ok(it) => {
                println!("Found ~/{}. Resuming ...", STATS_FILENAME);
                Ok(it)
            }
            Err(error) => {
                eprintln!("E: Error parsing ~/{}. Starting from scratch ...", STATS_FILENAME);
                Err(error)
            }
        }
    }

    fn create_recorder_from_stats_file(mut file: File, cores: usize) -> Result<StatsRecorder, Error> {
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        StatsRecorder::from_json(contents, Some(StatsRecorderFactory::try_update_stats), cores)
    }

    fn try_update_stats(recorder: &StatsRecorder) {
        match StatsRecorderFactory::update_stats(recorder) {
            Ok(_) => (),
            Err(_) => eprintln!("E: Failed to find, create, read or write ~/{}", STATS_FILENAME),
        }
    }

    fn update_stats(recorder: &StatsRecorder) -> Result<(), Error> {
        let mut file = File::create(StatsRecorderFactory::stats_path()?)?;
        file.write_all(recorder.to_json()?.as_bytes())?;
        file.flush()
    }
}

#[derive(Clone)]
pub struct StatsRecorder {
    pub stats: Stats,
    pub nnue_nps: NpsRecorder,
    on_batch_recorded: OnBatchRecordedCallback,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Stats {
    pub total_batches: u64,
    pub total_positions: u64,
    pub total_nodes: u64,
}

impl StatsRecorder {
    fn new(cores: usize, on_batch_recorded: OnBatchRecordedCallback) -> StatsRecorder {
        StatsRecorder {
            stats: Stats {
                total_batches: 0,
                total_positions: 0,
                total_nodes: 0,
            },
            nnue_nps: NpsRecorder::new(cores),
            on_batch_recorded,
        }
    }

    pub fn record_batch(&mut self, positions: u64, nodes: u64, nnue_nps: Option<u32>) {
        self.stats.total_batches += 1;
        self.stats.total_positions += positions;
        self.stats.total_nodes += nodes;
        if let Some(nnue_nps) = nnue_nps {
            self.nnue_nps.record(nnue_nps);
        }

        self.on_batch_recorded.map(|callback| callback(self));
    }

    pub fn min_user_backlog(&self) -> Duration {
        // The average batch has 60 positions, analysed with 2_250_000 nodes
        // each. Top end clients take no longer than 35 seconds.
        let best_batch_seconds = 35;

        // Estimate how long this client would take for the next batch,
        // capped at timeout.
        let estimated_batch_seconds = u64::from(min(6 * 60, 60 * 2_250_000 / max(1, self.nnue_nps.nps)));

        // Its worth joining if queue wait time + estimated time < top client
        // time on empty queue.
        Duration::from_secs(estimated_batch_seconds.saturating_sub(best_batch_seconds))
    }

    fn to_json(&self) -> Result<String, Error> {
        Ok(serde_json::to_string_pretty(&self.stats)?)
    }

    fn from_json(json: String, on_batch_recorded: OnBatchRecordedCallback, cores: usize) -> Result<StatsRecorder, Error> {
        let stats: Stats = serde_json::from_str(&json)?;
        Ok(StatsRecorder {
            stats,
            nnue_nps: NpsRecorder::new(cores),
            on_batch_recorded,
        })
    }
}

#[derive(Clone)]
pub struct NpsRecorder {
    pub nps: u32,
    pub uncertainty: f64,
}

impl NpsRecorder {
    fn new(cores: usize) -> NpsRecorder {
        NpsRecorder {
            nps: 500_000 * cores as u32, // start with a low estimate
            uncertainty: 1.0,
        }
    }

    fn record(&mut self, nps: u32) {
        let alpha = 0.9;
        self.uncertainty *= alpha;
        self.nps = (f64::from(self.nps) * alpha + f64::from(nps) * (1.0 - alpha)) as u32;
    }
}

impl fmt::Display for NpsRecorder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} knps", self.nps / 1000)?;
        if self.uncertainty > 0.7 {
            write!(f, "?")?;
        }
        if self.uncertainty > 0.4 {
            write!(f, "?")?;
        }
        if self.uncertainty > 0.1 {
            write!(f, "?")?;
        }
        Ok(())
    }
}
