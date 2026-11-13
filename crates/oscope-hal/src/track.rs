//! The stage's position track: its recent position samples, and where it
//! was at any moment between them.
//!
//! A stage driver pushes each report as it arrives. Acquisition asks where
//! the stage was at a frame's exposure midpoint (plan §5.5), and so do
//! scans and calibrations.

use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard, PoisonError};

use oscope_core::{Timestamp, Um, Xyz};

use crate::PositionSample;

/// The latest position samples, in time order, in a ring of fixed size.
/// It is shared between threads: one pushes, any number read.
#[derive(Debug)]
pub struct PositionTrack {
    capacity: usize,
    samples: Mutex<VecDeque<PositionSample>>,
}

impl PositionTrack {
    /// A track that keeps the latest `capacity` samples, and at least 2.
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(2);
        Self {
            capacity,
            samples: Mutex::new(VecDeque::with_capacity(capacity)),
        }
    }

    fn samples(&self) -> MutexGuard<'_, VecDeque<PositionSample>> {
        // A panic elsewhere can't leave the ring half-changed, so a poisoned
        // lock is still good.
        self.samples.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// How many samples it keeps.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// How many samples it holds.
    pub fn len(&self) -> usize {
        self.samples().len()
    }

    /// Whether it holds none.
    pub fn is_empty(&self) -> bool {
        self.samples().is_empty()
    }

    /// Adds a sample, dropping the oldest when the ring is full.
    ///
    /// Samples come in time order. One older than the latest is ignored, and
    /// `false` returned; one at the latest's time replaces it.
    pub fn push(&self, sample: PositionSample) -> bool {
        let mut samples = self.samples();
        match samples.back() {
            Some(latest) if sample.time < latest.time => return false,
            Some(latest) if sample.time == latest.time => {
                samples.pop_back();
            }
            _ => {}
        }
        if samples.len() == self.capacity {
            samples.pop_front();
        }
        samples.push_back(sample);
        true
    }

    /// The latest sample.
    pub fn latest(&self) -> Option<PositionSample> {
        self.samples().back().copied()
    }

    /// The oldest sample it still holds.
    pub fn oldest(&self) -> Option<PositionSample> {
        self.samples().front().copied()
    }

    /// Where the stage was at `time`: linearly interpolated between the
    /// samples on either side of it, or a sample's own position at its time.
    ///
    /// `None` before the oldest sample, and after the latest: the track
    /// never guesses ahead, so a caller that needs a moment after the latest
    /// report waits for the next one.
    pub fn at(&self, time: Timestamp) -> Option<Xyz> {
        let samples = self.samples();
        let first = samples.front()?;
        let last = samples.back()?;
        if time < first.time || time > last.time {
            return None;
        }
        // The first sample at or after `time`.
        let after = samples.partition_point(|s| s.time < time);
        let b = samples[after];
        if b.time == time || after == 0 {
            return Some(b.position);
        }
        let a = samples[after - 1];
        let span = (b.time.0 - a.time.0) as f64;
        let f = (time.0 - a.time.0) as f64 / span;
        let lerp = |p: Um, q: Um| Um(p.0 + (q.0 - p.0) * f);
        Some(Xyz::new(
            lerp(a.position.x, b.position.x),
            lerp(a.position.y, b.position.y),
            lerp(a.position.z, b.position.z),
        ))
    }
}
