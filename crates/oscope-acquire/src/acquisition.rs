//! Acquisition: a camera's frames, completed with what the camera doesn't
//! know, then published on the bus (ADR-0012 §3; P16).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use oscope_core::{Frame, LightState, timing};
use oscope_hal::{FrameSink, PositionTrack};

use crate::bus::FrameBus;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What a frame's metadata needs from the rest of the instrument.
#[derive(Default)]
struct Context {
    track: Option<Arc<PositionTrack>>,
    objective: Option<String>,
    lights: Vec<LightState>,
}

struct Inner {
    bus: Arc<FrameBus>,
    context: Mutex<Context>,
    wait: Duration,
    published: AtomicU64,
    unplaced: AtomicU64,
    refused: AtomicU64,
}

/// Completes a camera's frames and publishes them. For each frame it fills:
///
/// - **the position:** the stage's position track at the exposure's
///   midpoint, interpolated, never guessed ahead. It waits, up to its
///   bound, for the stage to report after that midpoint, about a report
///   interval and its latency. With no report in time, the frame has no
///   position, and is counted;
/// - **the objective** fitted, by name;
/// - **the lights,** as they were.
///
/// Then it seals the frame and publishes it on the bus.
#[derive(Clone)]
pub struct Acquisition {
    inner: Arc<Inner>,
}

impl Acquisition {
    /// Publishes on `bus`, waiting up to `wait` for a stage report.
    pub fn new(bus: Arc<FrameBus>, wait: Duration) -> Self {
        Self {
            inner: Arc::new(Inner {
                bus,
                context: Mutex::new(Context::default()),
                wait,
                published: AtomicU64::new(0),
                unplaced: AtomicU64::new(0),
                refused: AtomicU64::new(0),
            }),
        }
    }

    /// The stage's position track, or none.
    pub fn set_track(&self, track: Option<Arc<PositionTrack>>) {
        lock(&self.inner.context).track = track;
    }

    /// The objective fitted, by name, or none.
    pub fn set_objective(&self, objective: Option<String>) {
        lock(&self.inner.context).objective = objective;
    }

    /// The lights, as they are now.
    pub fn set_lights(&self, lights: Vec<LightState>) {
        lock(&self.inner.context).lights = lights;
    }

    /// The bus it publishes on.
    pub fn bus(&self) -> &Arc<FrameBus> {
        &self.inner.bus
    }

    /// Frames published; of them, those without a position; and frames
    /// refused because their buffer was too small for their metadata.
    pub fn counts(&self) -> (u64, u64, u64) {
        let i = &self.inner;
        (
            i.published.load(Ordering::Relaxed),
            i.unplaced.load(Ordering::Relaxed),
            i.refused.load(Ordering::Relaxed),
        )
    }

    /// The sink to start a camera with.
    pub fn sink(&self) -> FrameSink {
        let inner = Arc::clone(&self.inner);
        Arc::new(move |buffer, mut meta| {
            let (track, objective, lights) = {
                let context = lock(&inner.context);
                (
                    context.track.clone(),
                    context.objective.clone(),
                    context.lights.clone(),
                )
            };
            if let Some(track) = track {
                let waiting = timing::start("acquisition.position_wait");
                let middle = meta.exposure_midpoint();
                let deadline = Instant::now() + inner.wait;
                while track.latest().is_none_or(|s| s.time < middle) && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(1));
                }
                meta.position = track.at(middle);
                waiting.stop();
            }
            let _publishing = timing::start("acquisition.publish");
            if meta.position.is_none() {
                inner.unplaced.fetch_add(1, Ordering::Relaxed);
            }
            meta.objective = objective;
            meta.lights = lights;
            match Frame::new(buffer, meta) {
                Ok(frame) => {
                    inner.bus.publish(frame);
                    inner.published.fetch_add(1, Ordering::Relaxed);
                }
                Err(_) => {
                    inner.refused.fetch_add(1, Ordering::Relaxed);
                }
            }
        })
    }
}
