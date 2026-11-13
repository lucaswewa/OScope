//! The simulation clock, shared by every simulated device, and OScope's own
//! clock whenever the simulator is in use (ADR-0007, ADR-0013).

use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use oscope_core::{Clock, Timestamp};
use tokio::sync::Notify;

/// How fast simulated time runs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Rate {
    /// At the host's pace, on its monotonic clock: the default.
    RealTime,
    /// This many times faster than the host's pace (slower, below 1).
    Scaled(f64),
    /// Only when a test advances it.
    Stepped,
}

impl Rate {
    /// Simulated seconds for each host second: 0 when stepped.
    fn factor(self) -> f64 {
        match self {
            Rate::RealTime => 1.0,
            Rate::Scaled(k) => k.max(0.0),
            Rate::Stepped => 0.0,
        }
    }
}

#[derive(Debug)]
struct State {
    rate: Rate,
    /// Simulated time at `host`, in nanoseconds.
    base: u64,
    host: Instant,
}

impl State {
    fn now(&self) -> u64 {
        let k = self.rate.factor();
        if k == 0.0 {
            return self.base;
        }
        let elapsed = self.host.elapsed().as_secs_f64() * k * 1e9;
        self.base.saturating_add(elapsed as u64)
    }

    /// Restarts from now, so that a change of rate doesn't move the time.
    fn rebase(&mut self) {
        self.base = self.now();
        self.host = Instant::now();
    }
}

/// One clock for the whole simulator.
///
/// - **At 1×** it follows the host's monotonic clock; **scaled,** it runs k
///   times as fast; **stepped,** it moves only by [`SimClock::advance`].
/// - **Changing its rate** never moves it, and it never goes back.
/// - **[`SimClock::sleep_until`]** waits for a simulated moment, at any rate.
#[derive(Debug)]
pub struct SimClock {
    state: Mutex<State>,
    start_utc: DateTime<Utc>,
    changed: Notify,
}

impl SimClock {
    /// A clock at its start, running at `rate`.
    pub fn new(rate: Rate) -> Self {
        Self {
            state: Mutex::new(State {
                rate,
                base: 0,
                host: Instant::now(),
            }),
            start_utc: Utc::now(),
            changed: Notify::new(),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Its rate.
    pub fn rate(&self) -> Rate {
        self.state().rate
    }

    /// Changes its rate from now on.
    pub fn set_rate(&self, rate: Rate) {
        let mut state = self.state();
        state.rebase();
        state.rate = rate;
        drop(state);
        self.changed.notify_waiters();
    }

    /// Moves it on by `by`, at any rate.
    pub fn advance(&self, by: Duration) {
        let mut state = self.state();
        state.rebase();
        state.base = state.base.saturating_add(by.as_nanos() as u64);
        drop(state);
        self.changed.notify_waiters();
    }

    /// Moves it on to `at`, if `at` is later than now.
    pub fn advance_to(&self, at: Timestamp) {
        let now = self.now();
        if at > now {
            self.advance(Duration::from_nanos(at.0 - now.0));
        }
    }

    /// Waits until the clock reaches `at`.
    ///
    /// At 1× or scaled, it sleeps on the host's timer for the simulated time
    /// left, divided by the rate, and sleeps again if the rate changes.
    /// Stepped, it wakes when [`SimClock::advance`] passes `at`.
    pub async fn sleep_until(&self, at: Timestamp) {
        loop {
            // Listening before looking, so that no change is missed.
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let (now, k) = {
                let state = self.state();
                (state.now(), state.rate.factor())
            };
            if now >= at.0 {
                return;
            }
            if k == 0.0 {
                changed.await;
            } else {
                let left = Duration::from_secs_f64((at.0 - now) as f64 / 1e9 / k);
                tokio::select! {
                    () = tokio::time::sleep(left) => {}
                    () = &mut changed => {}
                }
            }
        }
    }

    /// Waits for `by` of simulated time.
    pub async fn sleep(&self, by: Duration) {
        self.sleep_until(self.now() + by).await;
    }
}

impl Clock for SimClock {
    fn now(&self) -> Timestamp {
        Timestamp(self.state().now())
    }

    fn start_utc(&self) -> DateTime<Utc> {
        self.start_utc
    }
}
