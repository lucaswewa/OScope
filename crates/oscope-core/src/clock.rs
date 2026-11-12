//! OScope's one clock (ADR-0007): a monotonic [`Timestamp`] in nanoseconds
//! since the clock started, mapped once to UTC.
//!
//! Every timestamp in a process comes from one [`Clock`], shared as
//! `Arc<dyn Clock>`. Normally that is a [`SystemClock`]. With the simulator,
//! it is the simulation clock (P11), and in tests a [`FakeClock`].

use std::fmt;
use std::ops::{Add, Sub};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use chrono::{DateTime, TimeDelta, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A moment on OScope's clock: nanoseconds since the clock started.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(transparent)]
pub struct Timestamp(pub u64);

impl Timestamp {
    /// The moment the clock started.
    pub const START: Self = Self(0);

    /// The time from the clock's start to this moment.
    pub fn since_start(self) -> Duration {
        Duration::from_nanos(self.0)
    }

    /// The time from `earlier` to this moment, or `None` if `earlier` is later.
    pub fn checked_since(self, earlier: Timestamp) -> Option<Duration> {
        self.0.checked_sub(earlier.0).map(Duration::from_nanos)
    }

    /// The time from `earlier` to this moment, or zero if `earlier` is later.
    pub fn saturating_since(self, earlier: Timestamp) -> Duration {
        Duration::from_nanos(self.0.saturating_sub(earlier.0))
    }

    /// The moment halfway between two, such as an exposure's midpoint.
    pub fn midpoint(self, other: Timestamp) -> Timestamp {
        Timestamp(self.0.midpoint(other.0))
    }
}

fn nanos(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

/// Later by a duration, saturating at the end of time (in 584 years).
impl Add<Duration> for Timestamp {
    type Output = Timestamp;
    fn add(self, duration: Duration) -> Timestamp {
        Timestamp(self.0.saturating_add(nanos(duration)))
    }
}

/// Earlier by a duration, saturating at the clock's start.
impl Sub<Duration> for Timestamp {
    type Output = Timestamp;
    fn sub(self, duration: Duration) -> Timestamp {
        Timestamp(self.0.saturating_sub(nanos(duration)))
    }
}

/// Seconds since the start, to the nanosecond: `12.000345678 s`.
impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}.{:09} s",
            self.0 / 1_000_000_000,
            self.0 % 1_000_000_000
        )
    }
}

/// A source of [`Timestamp`]s.
pub trait Clock: Send + Sync {
    /// Now. Never earlier than what an earlier call returned.
    fn now(&self) -> Timestamp;

    /// The UTC time of [`Timestamp::START`], fixed when the clock started.
    fn start_utc(&self) -> DateTime<Utc>;

    /// The UTC time of a moment: the start's, plus the time since it.
    fn utc(&self, at: Timestamp) -> DateTime<Utc> {
        self.start_utc() + TimeDelta::nanoseconds(i64::try_from(at.0).unwrap_or(i64::MAX))
    }
}

/// The host's monotonic clock ([`Instant`]), mapped to UTC when it starts.
#[derive(Debug)]
pub struct SystemClock {
    start: Instant,
    start_utc: DateTime<Utc>,
}

impl SystemClock {
    /// A clock that starts now.
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
            start_utc: Utc::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp(nanos(self.start.elapsed()))
    }

    fn start_utc(&self) -> DateTime<Utc> {
        self.start_utc
    }
}

/// A clock that moves only when told to: for tests.
#[derive(Debug)]
pub struct FakeClock {
    now: AtomicU64,
    start_utc: DateTime<Utc>,
}

impl FakeClock {
    /// A clock at its start, which was at `start_utc`.
    pub fn new(start_utc: DateTime<Utc>) -> Self {
        Self {
            now: AtomicU64::new(0),
            start_utc,
        }
    }

    /// Moves the clock on by `by`.
    pub fn advance(&self, by: Duration) {
        let by = nanos(by);
        // The closure always returns `Some`, so the update can't fail.
        let _ = self
            .now
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |now| {
                Some(now.saturating_add(by))
            });
    }

    /// Moves the clock on to `at`. A clock never goes back, so an earlier
    /// `at` changes nothing.
    pub fn advance_to(&self, at: Timestamp) {
        self.now.fetch_max(at.0, Ordering::SeqCst);
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Timestamp {
        Timestamp(self.now.load(Ordering::SeqCst))
    }

    fn start_utc(&self) -> DateTime<Utc> {
        self.start_utc
    }
}
