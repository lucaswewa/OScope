//! Step timings, for profiling the live path: how long each named step took,
//! across the process, over the last [`WINDOW`]. `ex-live-view` reads them
//! for its benchmark.
//!
//! Timing is off until [`enable`] turns it on, so a step's [`Timer`] costs
//! one atomic load until then.
//!
//! ```
//! use oscope_core::timing;
//!
//! timing::enable(true);
//! let sum = timing::time("example.sum", || (1..=100).sum::<u32>());
//! assert_eq!(sum, 5050);
//! let steps = timing::snapshot();
//! assert!(steps.iter().any(|s| s.step == "example.sum" && s.count == 1));
//! ```

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use serde::Serialize;

/// How long samples are kept.
pub const WINDOW: Duration = Duration::from_secs(30);
/// The most samples kept for each step.
const MOST: usize = 4096;

type Samples = VecDeque<(Instant, Duration)>;

static ON: AtomicBool = AtomicBool::new(false);
static STEPS: Mutex<BTreeMap<&'static str, Samples>> = Mutex::new(BTreeMap::new());

fn steps() -> MutexGuard<'static, BTreeMap<&'static str, Samples>> {
    STEPS.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Turns timing on or off, for the whole process.
pub fn enable(on: bool) {
    ON.store(on, Ordering::Relaxed);
}

/// Whether timing is on.
pub fn enabled() -> bool {
    ON.load(Ordering::Relaxed)
}

/// Records that `step` took `took`, if timing is on.
pub fn record(step: &'static str, took: Duration) {
    if !enabled() {
        return;
    }
    let now = Instant::now();
    let mut steps = steps();
    let samples = steps.entry(step).or_default();
    samples.push_back((now, took));
    while samples.len() > MOST || samples.front().is_some_and(|(at, _)| now - *at > WINDOW) {
        samples.pop_front();
    }
}

/// Times `step` from now until the timer is dropped or stopped.
#[must_use = "a timer records its step when it is dropped"]
pub fn start(step: &'static str) -> Timer {
    Timer {
        step,
        started: enabled().then(Instant::now),
    }
}

/// `work`, timed as `step`.
pub fn time<T>(step: &'static str, work: impl FnOnce() -> T) -> T {
    let _timer = start(step);
    work()
}

/// A step being timed: see [`start`].
#[derive(Debug)]
pub struct Timer {
    step: &'static str,
    started: Option<Instant>,
}

impl Timer {
    /// Ends the step now.
    pub fn stop(self) {}
}

impl Drop for Timer {
    fn drop(&mut self) {
        if let Some(started) = self.started {
            record(self.step, started.elapsed());
        }
    }
}

/// How long a step took over the last [`WINDOW`].
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StepStats {
    /// Its name, such as `preview.jpeg`.
    pub step: String,
    /// How many times it ran.
    pub count: usize,
    /// How many times a second, from its first sample to its last.
    pub per_s: f64,
    /// The mean time it took, in ms.
    pub mean_ms: f64,
    /// The median.
    pub p50_ms: f64,
    /// The 95th percentile.
    pub p95_ms: f64,
    /// The longest.
    pub max_ms: f64,
}

/// Every step that ran in the last [`WINDOW`], by name.
pub fn snapshot() -> Vec<StepStats> {
    let now = Instant::now();
    let steps = steps();
    steps
        .iter()
        .filter_map(|(step, samples)| {
            let kept: Vec<_> = samples
                .iter()
                .filter(|(at, _)| now - *at <= WINDOW)
                .collect();
            let (first, last) = (kept.first()?.0, kept.last()?.0);
            let mut ms: Vec<f64> = kept.iter().map(|(_, d)| d.as_secs_f64() * 1e3).collect();
            ms.sort_by(f64::total_cmp);
            let rank = |q: f64| ms[((ms.len() - 1) as f64 * q).round() as usize];
            let span = (last - first).as_secs_f64();
            Some(StepStats {
                step: (*step).to_owned(),
                count: ms.len(),
                per_s: match span > 0.0 {
                    true => (ms.len() - 1) as f64 / span,
                    false => 0.0,
                },
                mean_ms: ms.iter().sum::<f64>() / ms.len() as f64,
                p50_ms: rank(0.5),
                p95_ms: rank(0.95),
                max_ms: ms[ms.len() - 1],
            })
        })
        .collect()
}

/// Forgets every sample.
pub fn reset() {
    steps().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_are_counted_only_while_timing_is_on() {
        enable(false);
        time("test.off", || ());
        enable(true);
        for ms in [1, 2, 3, 4, 10] {
            record("test.on", Duration::from_millis(ms));
        }
        let steps = snapshot();
        assert!(!steps.iter().any(|s| s.step == "test.off"));
        let on = steps.iter().find(|s| s.step == "test.on").unwrap();
        assert_eq!(on.count, 5);
        assert!((on.mean_ms - 4.0).abs() < 1e-9);
        assert!((on.p50_ms - 3.0).abs() < 1e-9);
        assert!((on.max_ms - 10.0).abs() < 1e-9);
    }
}
