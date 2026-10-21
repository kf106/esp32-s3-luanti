//! The game's clock: the time of day, and how long the world has run,
//! kept over restarts in `world/env_meta.txt` as Luanti keeps them
//! (ServerEnvironment::saveMeta: "game_time" and "time_of_day" settings,
//! ending with "EnvArgsEnd").
//!
//! The board has no clock that survives power-off, and is usually stopped
//! by pulling the plug, so the file is rewritten every SAVE_INTERVAL; at
//! most that much game time is lost.

use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use log::{info, warn};

use crate::storage;

/// A new world starts at 10:00.
pub const NEW_WORLD_TIME: u16 = 10000;
/// Game time runs this many times faster than real time (Luanti's
/// time_speed default): a day lasts 20 minutes.
pub const TIME_SPEED: f32 = 72.0;
const SAVE_INTERVAL: Duration = Duration::from_secs(60);

pub struct Clock {
    path: Option<PathBuf>,
    /// The time of day (0..24000) and game time (seconds) when this run
    /// started.
    start_time: u16,
    start_game_time: u64,
    started: Instant,
    saved_at: Instant,
}

impl Clock {
    /// The clock saved at `path`, or a new world's if there is none.
    pub fn open(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let (time, game_time) = match storage::read(&path) {
            Ok(text) => parse(&String::from_utf8_lossy(&text)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => (None, None),
            Err(e) => {
                warn!("Reading {}: {e}", path.display());
                (None, None)
            }
        };
        let clock = Self {
            path: Some(path),
            start_time: time.unwrap_or(NEW_WORLD_TIME),
            start_game_time: game_time.unwrap_or(0),
            started: Instant::now(),
            saved_at: Instant::now(),
        };
        match time {
            Some(_) => info!("The time is {} (world age {} s)", clock.hh_mm(), clock.game_time()),
            None => info!("New world: the time is {}", clock.hh_mm()),
        }
        clock
    }

    /// The time of day in Luanti's units (0..24000, 6000 = 06:00).
    pub fn time_of_day(&self) -> u16 {
        time_after(self.start_time, self.started.elapsed().as_secs_f64())
    }

    /// Seconds the world has run, over all restarts (Luanti's game_time).
    pub fn game_time(&self) -> u64 {
        self.start_game_time + self.started.elapsed().as_secs()
    }

    /// The time of day as hh:mm.
    pub fn hh_mm(&self) -> String {
        let minutes = self.time_of_day() as u32 * 1440 / 24000;
        format!("{}:{:02}", minutes / 60, minutes % 60)
    }

    /// Save, if SAVE_INTERVAL has passed since the last time.
    pub fn save_if_due(&mut self) {
        if self.saved_at.elapsed() >= SAVE_INTERVAL {
            self.save();
        }
    }

    pub fn save(&mut self) {
        self.saved_at = Instant::now();
        let Some(path) = &self.path else {
            return;
        };
        let text = format!("game_time = {}\ntime_of_day = {}\nEnvArgsEnd\n", self.game_time(), self.time_of_day());
        if let Err(e) = storage::write(path, text.as_bytes()) {
            warn!("Saving {}: {e}", path.display());
            crate::status::problem();
        }
    }
}

/// The time of day `secs` real seconds after `start`.
fn time_after(start: u16, secs: f64) -> u16 {
    ((start as f64 + secs * TIME_SPEED as f64 * 24000.0 / 86400.0) % 24000.0) as u16
}

/// The time of day and game time from Luanti's env_meta.txt settings.
fn parse(text: &str) -> (Option<u16>, Option<u64>) {
    let mut time = None;
    let mut game_time = None;
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "time_of_day" => time = value.trim().parse::<u32>().ok().map(|t| (t % 24000) as u16),
            "game_time" => game_time = value.trim().parse().ok(),
            _ => {}
        }
    }
    (time, game_time)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeping_time() {
        // Twenty real minutes make a day.
        assert_eq!(time_after(10000, 0.0), 10000);
        assert_eq!(time_after(10000, 600.0), 22000);
        assert_eq!(time_after(10000, 1200.0), 10000);
        assert_eq!(parse("game_time = 5000\ntime_of_day = 18500\nEnvArgsEnd\n"), (Some(18500), Some(5000)));
        assert_eq!(parse("time_of_day = 30000\n"), (Some(6000), None));
        assert_eq!(parse("rubbish\n"), (None, None));

        let dir = std::env::temp_dir().join(format!("esp_clock_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("env_meta.txt");
        let _ = std::fs::remove_file(&path);
        let mut c = Clock::open(&path);
        assert_eq!(c.time_of_day(), NEW_WORLD_TIME);
        c.start_time = 18500;
        c.start_game_time = 5000;
        c.save();
        let again = Clock::open(&path);
        assert_eq!((again.time_of_day(), again.game_time()), (18500, 5000));
        assert_eq!(again.hh_mm(), "18:30");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
