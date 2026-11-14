//! The fake camera.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use oscope_core::{
    BayerPattern, Clock, FrameId, FrameMeta, FramePool, Ms, PixelFormat, Timestamp, Um,
};

use super::Script;
use crate::{
    Camera, CameraCapabilities, CameraMode, CameraSettings, Error, FrameSink, HalFuture, ModeRate,
    RoiRules, Shutter, Trigger, TriggerCapabilities,
};

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A camera that streams frames of a test pattern, on a thread of its own.
///
/// - **The pattern** is horizontal stripes that move down a row with each
///   frame: row `y` of frame `n` holds `(y + n) mod 256` in every sample,
///   times 257 in 16 bits.
/// - **Free-running,** it delivers a frame at its mode's rate, or once each
///   exposure if that is longer, on a fixed schedule: a frame whose time
///   passes while the sink holds its thread is lost, and the next frame's
///   `dropped_before` counts it. **Under a software trigger,** it delivers
///   one for each [`Camera::trigger`]. An external trigger never comes.
/// - **When the pool has no free slab,** the frame is lost, and the next
///   frame's `dropped_before` counts it.
/// - **`stop`** waits for the thread to end, at most one frame's interval.
pub struct FakeCamera {
    capabilities: CameraCapabilities,
    clock: Arc<dyn Clock>,
    script: Script,
    settings: Arc<Mutex<CameraSettings>>,
    stream: Mutex<Option<Stream>>,
}

struct Stream {
    stop: Arc<AtomicBool>,
    triggers: Sender<()>,
    thread: JoinHandle<()>,
}

impl FakeCamera {
    /// A fake with [`FakeCamera::default_capabilities`].
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self::with_capabilities(Self::default_capabilities(), clock)
    }

    /// A 640 × 480 RGGB sensor of 2.4 µm pixels, with a rolling shutter:
    /// - modes `full` (100 fps in 8 bits, 50 in 16) and `bin2` (200 and
    ///   100);
    /// - ROIs in steps of 2, of at least 16 pixels, which don't raise the
    ///   rate;
    /// - a software trigger, and a temperature of 25 °C.
    pub fn default_capabilities() -> CameraCapabilities {
        let rates = |fps8: f64| {
            vec![
                ModeRate {
                    format: PixelFormat::Bayer8 {
                        pattern: BayerPattern::Rggb,
                    },
                    fps: fps8,
                },
                ModeRate {
                    format: PixelFormat::Bayer16 {
                        pattern: BayerPattern::Rggb,
                        significant_bits: 16,
                    },
                    fps: fps8 / 2.0,
                },
            ]
        };
        let mode = |name: &str, binning: u32, fps8: f64| CameraMode {
            name: name.to_owned(),
            width: 640 / binning,
            height: 480 / binning,
            binning,
            rates: rates(fps8),
            line_time: Some(Ms(0.01)),
        };
        CameraCapabilities {
            model: "fake camera".to_owned(),
            sensor_width: 640,
            sensor_height: 480,
            pixel_size: Um(2.4),
            shutter: Shutter::Rolling,
            modes: vec![mode("full", 1, 100.0), mode("bin2", 2, 200.0)],
            exposure_min: Ms(0.02),
            exposure_max: Ms(10_000.0),
            gain_min: 0.0,
            gain_max: 100.0,
            roi: Some(RoiRules {
                x_step: 2,
                y_step: 2,
                min_size: 16,
                raises_rate: false,
            }),
            trigger: TriggerCapabilities {
                software: true,
                external: false,
                output: false,
            },
            sensor_timestamps: false,
            temperature: true,
        }
    }

    /// A fake with these capabilities, which must have a mode. It starts in
    /// its first mode and format, free-running, with a 1 ms exposure (or the
    /// nearest it allows), the least gain, and no ROI.
    pub fn with_capabilities(capabilities: CameraCapabilities, clock: Arc<dyn Clock>) -> Self {
        let mode = &capabilities.modes[0];
        let settings = CameraSettings {
            mode: mode.name.clone(),
            format: mode.rates[0].format,
            exposure: Ms(1.0f64.clamp(capabilities.exposure_min.0, capabilities.exposure_max.0)),
            gain: capabilities.gain_min,
            roi: None,
            trigger: Trigger::FreeRunning,
        };
        Self {
            capabilities,
            clock,
            script: Script::default(),
            settings: Arc::new(Mutex::new(settings)),
            stream: Mutex::new(None),
        }
    }

    /// Its script.
    pub fn script(&self) -> &Script {
        &self.script
    }

    fn halt(&self) {
        if let Some(stream) = lock(&self.stream).take() {
            stream.stop.store(true, Ordering::Release);
            // A panic on the thread has already been reported.
            let _ = stream.thread.join();
        }
    }
}

impl Drop for FakeCamera {
    fn drop(&mut self) {
        self.halt();
    }
}

/// What the streaming thread needs.
struct Run {
    capabilities: CameraCapabilities,
    settings: Arc<Mutex<CameraSettings>>,
    clock: Arc<dyn Clock>,
    pool: FramePool,
    sink: FrameSink,
    stop: Arc<AtomicBool>,
    triggers: Receiver<()>,
}

impl Run {
    fn go(self) {
        let first = lock(&self.settings).clone();
        let Ok(mode) = self.capabilities.mode(&first.mode) else {
            return;
        };
        let fps = mode.fps(first.format).unwrap_or(1.0);
        let Ok((roi, width, height)) = self.capabilities.frame_shape(&first) else {
            return;
        };
        let (mut id, mut dropped, mut next) = (FrameId(0), 0, Instant::now());
        while !self.stop.load(Ordering::Acquire) {
            let now = lock(&self.settings).clone();
            let exposure = now.exposure.to_duration().unwrap_or_default();
            let due = match first.trigger {
                Trigger::FreeRunning => {
                    // A fixed schedule: the frames whose time passed while
                    // the sink held this thread are lost.
                    let period = exposure.max(Duration::from_secs_f64(1.0 / fps));
                    next += period;
                    let at = Instant::now();
                    match next.checked_duration_since(at) {
                        Some(wait) => thread::sleep(wait),
                        None => {
                            let missed = (at - next).as_secs_f64() / period.as_secs_f64();
                            dropped += missed as u64;
                            next += period.mul_f64(missed.floor());
                        }
                    }
                    true
                }
                Trigger::Software | Trigger::External { .. } => {
                    match self.triggers.recv_timeout(Duration::from_millis(5)) {
                        Ok(()) => true,
                        Err(RecvTimeoutError::Timeout) => false,
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                }
            };
            if !due || self.stop.load(Ordering::Acquire) {
                continue;
            }
            let Some(mut buffer) = self.pool.take() else {
                dropped += 1;
                continue;
            };
            // A row's level, times 257 in 16 bits, is the level in each of a
            // sample's bytes: a row is its level in every byte.
            let row = first.format.pixel_bytes() * width as usize;
            for (y, line) in buffer.bytes_mut()[..row * height as usize]
                .chunks_exact_mut(row)
                .enumerate()
            {
                line.fill(((y as u64 + id.0) % 256) as u8);
            }
            let host_time = self.clock.now();
            let mut meta = FrameMeta::new(id, first.format, width, height, host_time);
            meta.roi = roi;
            meta.binning = mode.binning;
            meta.mode = mode.name.clone();
            meta.exposure = now.exposure;
            meta.exposure_start = Timestamp(host_time.0.saturating_sub(exposure.as_nanos() as u64));
            meta.line_time = mode.line_time;
            meta.gain = now.gain;
            meta.dropped_before = dropped;
            dropped = 0;
            (self.sink)(buffer, meta);
            id = id.next();
        }
    }
}

impl Camera for FakeCamera {
    fn capabilities(&self) -> &CameraCapabilities {
        &self.capabilities
    }

    fn settings(&self) -> HalFuture<'_, CameraSettings> {
        Box::pin(async move {
            self.script.call("settings")?;
            Ok(lock(&self.settings).clone())
        })
    }

    fn configure(&self, settings: CameraSettings) -> HalFuture<'_, CameraSettings> {
        Box::pin(async move {
            self.script.call("configure")?;
            let applied = self.capabilities.check(&settings)?;
            let streaming = self.streaming();
            let mut current = lock(&self.settings);
            let shape = |s: &CameraSettings| (s.mode.clone(), s.format, s.roi, s.trigger);
            if streaming && shape(&applied) != shape(&current) {
                return Err(Error::Busy {
                    reason: "the mode, format, ROI and trigger can't change while it streams"
                        .to_owned(),
                });
            }
            *current = applied.clone();
            Ok(applied)
        })
    }

    fn start(&self, pool: FramePool, sink: FrameSink) -> HalFuture<'_, ()> {
        Box::pin(async move {
            self.script.call("start")?;
            let mut stream = lock(&self.stream);
            if stream.is_some() {
                return Err(Error::Busy {
                    reason: "it is already streaming".to_owned(),
                });
            }
            let settings = lock(&self.settings).clone();
            let (_, width, height) = self.capabilities.frame_shape(&settings)?;
            let needed = settings.format.frame_bytes(width, height);
            Error::check_range(
                "the pool's slab size in bytes",
                pool.stats().slab_len as f64,
                needed as f64,
                f64::INFINITY,
            )?;
            let stop = Arc::new(AtomicBool::new(false));
            let (triggers, received) = mpsc::channel();
            let run = Run {
                capabilities: self.capabilities.clone(),
                settings: Arc::clone(&self.settings),
                clock: Arc::clone(&self.clock),
                pool,
                sink,
                stop: Arc::clone(&stop),
                triggers: received,
            };
            let thread = thread::Builder::new()
                .name("fake camera".to_owned())
                .spawn(move || run.go())
                .map_err(|e| Error::Device {
                    message: e.to_string(),
                })?;
            *stream = Some(Stream {
                stop,
                triggers,
                thread,
            });
            Ok(())
        })
    }

    fn stop(&self) -> HalFuture<'_, ()> {
        Box::pin(async move {
            self.script.call("stop")?;
            self.halt();
            Ok(())
        })
    }

    fn streaming(&self) -> bool {
        lock(&self.stream).is_some()
    }

    fn trigger(&self) -> HalFuture<'_, ()> {
        Box::pin(async move {
            self.script.call("trigger")?;
            if !self.capabilities.trigger.software {
                return Err(Error::Unsupported {
                    what: "a software trigger".to_owned(),
                });
            }
            let stream = lock(&self.stream);
            let busy = |reason: &str| Error::Busy {
                reason: reason.to_owned(),
            };
            let stream = stream.as_ref().ok_or_else(|| busy("it isn't streaming"))?;
            if lock(&self.settings).trigger != Trigger::Software {
                return Err(busy("it isn't set to a software trigger"));
            }
            stream.triggers.send(()).map_err(|_| Error::Disconnected)
        })
    }

    fn temperature(&self) -> HalFuture<'_, f64> {
        Box::pin(async move {
            self.script.call("temperature")?;
            if self.capabilities.temperature {
                Ok(25.0)
            } else {
                Err(Error::Unsupported {
                    what: "a temperature reading".to_owned(),
                })
            }
        })
    }
}
