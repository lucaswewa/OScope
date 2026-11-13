//! Live mode: a recipe on every frame, within a latency budget (ADR-0017).

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use oscope_compute::Lane;
use serde::Serialize;

use crate::plan::Plan;
use crate::run::{Engine, Run, RunError, RunOptions, lock};
use crate::value::Values;

/// A plan run on frames as they come, one at a time, in the live lane, and
/// never memoised. Frames don't queue: a frame still waiting when a newer
/// one comes is dropped for it, and one older than the budget when its
/// turn comes is dropped as late. A result that took longer than the
/// budget is still given, and counted.
pub struct Live {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

/// How live mode is doing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct LiveStats {
    /// Frames run, and their results given.
    pub delivered: u64,
    /// Frames dropped for a newer one before their turn came.
    pub superseded: u64,
    /// Frames dropped as older than the budget when their turn came.
    pub late: u64,
    /// Results given after more than the budget.
    pub over_budget: u64,
    /// Frames whose run failed.
    pub failed: u64,
    /// The latest result's latency, from its frame's capture.
    pub latency: Option<Duration>,
}

/// A frame's result.
#[derive(Debug)]
pub struct LiveResult {
    /// The run, or why it failed.
    pub run: Result<Run, RunError>,
    /// The time from the frame's capture to its result.
    pub latency: Duration,
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}

#[derive(Default)]
struct State {
    frame: Option<(Values, Instant)>,
    stop: bool,
    stats: LiveStats,
}

impl Live {
    /// Starts running `plan`, on a thread of its own, on each frame
    /// [`push`](Self::push)ed, giving each result to `deliver` on that
    /// thread, which should hand it on quickly. Refused unless every step
    /// can run live.
    pub fn start(
        engine: Arc<Engine>,
        plan: Arc<Plan>,
        budget: Duration,
        mut deliver: impl FnMut(LiveResult) + Send + 'static,
    ) -> Result<Live, RunError> {
        let refused = plan.not_live();
        if !refused.is_empty() {
            return Err(RunError::Unsuitable(format!(
                "{} can't run live",
                refused.join(", ")
            )));
        }
        let shared = Arc::new(Shared {
            state: Mutex::default(),
            wake: Condvar::new(),
        });
        let options = RunOptions {
            lane: Lane::Live,
            memoise: false,
            cancel: Arc::new(AtomicBool::new(false)),
        };
        let worker = std::thread::Builder::new()
            .name("oscope-live-recipe".into())
            .spawn({
                let shared = Arc::clone(&shared);
                move || {
                    while let Some((inputs, captured)) = shared.next(budget) {
                        let run = engine.run(&plan, &inputs, &options);
                        let latency = captured.elapsed();
                        {
                            let stats = &mut lock(&shared.state).stats;
                            match run {
                                Ok(_) => stats.delivered += 1,
                                Err(_) => stats.failed += 1,
                            }
                            stats.over_budget += u64::from(run.is_ok() && latency > budget);
                            stats.latency = Some(latency);
                        }
                        deliver(LiveResult { run, latency });
                    }
                }
            });
        let worker = worker
            .map_err(|error| RunError::Unsuitable(format!("live mode couldn't start: {error}")))?;
        Ok(Live {
            shared,
            worker: Some(worker),
        })
    }

    /// Offers a frame, captured at `captured`, by the recipe's input names.
    /// It never waits: a frame still waiting for its turn is dropped for
    /// this one.
    pub fn push(&self, inputs: Values, captured: Instant) {
        let mut state = lock(&self.shared.state);
        if state.frame.replace((inputs, captured)).is_some() {
            state.stats.superseded += 1;
        }
        self.shared.wake.notify_one();
    }

    /// How it is doing.
    pub fn stats(&self) -> LiveStats {
        lock(&self.shared.state).stats
    }
}

impl Shared {
    /// The next frame to run, waiting for one; frames older than `budget`
    /// are dropped as late. `None` once stopping.
    fn next(&self, budget: Duration) -> Option<(Values, Instant)> {
        let mut state = lock(&self.state);
        loop {
            if state.stop {
                return None;
            }
            match state.frame.take() {
                Some((_, captured)) if captured.elapsed() > budget => state.stats.late += 1,
                Some(frame) => return Some(frame),
                None => {
                    state = self
                        .wake
                        .wait(state)
                        .unwrap_or_else(PoisonError::into_inner)
                }
            }
        }
    }
}

impl Drop for Live {
    /// Stops it, once the frame it is running is done.
    fn drop(&mut self) {
        lock(&self.shared.state).stop = true;
        self.shared.wake.notify_one();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
