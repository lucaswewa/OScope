//! The simulated camera: a MIChrome 20 stand-in on the simulation clock
//! (P12, ADR-0013 §8).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use oscope_core::{
    BayerPattern, Clock, FrameId, FrameMeta, FramePool, Ms, PixelFormat, PixelRect, Point2, Rect,
    Timestamp, Um, timing,
};
use oscope_hal::{
    Camera, CameraCapabilities, CameraMode, CameraSettings, Error, FrameSink, HalFuture, ModeRate,
    RoiRules, Shutter, Trigger, TriggerCapabilities,
};
use rayon::prelude::*;

use crate::clock::SimClock;
#[cfg(feature = "cuda")]
use crate::gpu::GpuFrame;
use crate::noise::hash;
use crate::optics::Optics;
use crate::sensor::{Random, SensorModel};
use crate::stage::SimStage;
use crate::tiles::{CacheStats, TileCache};
use crate::world::{Light, World};

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The 8-bit raw mosaic.
pub const BAYER8: PixelFormat = PixelFormat::Bayer8 {
    pattern: BayerPattern::Rggb,
};
/// The 16-bit raw mosaic: 12 bits, aligned high.
pub const BAYER16: PixelFormat = PixelFormat::Bayer16 {
    pattern: BayerPattern::Rggb,
    significant_bits: 12,
};

/// How the sensor sits over the stage: the ground truth that the
/// camera-to-stage calibration (P32) has to recover.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mounting {
    /// The angle from the stage's x axis to the sensor's, in radians.
    pub rotation: f64,
    /// Whether the image is mirrored: the sensor's x reversed, before the
    /// turn.
    pub mirrored: bool,
    /// Where the sensor's centre looks, from the stage's position, in µm.
    pub offset: Point2,
}

impl Mounting {
    /// The stage point that a point `(dx, dy)` µm from the sensor's centre,
    /// along the sensor's axes, looks at, from the stage's position.
    pub fn to_stage(&self, dx: f64, dy: f64) -> Point2 {
        let dx = if self.mirrored { -dx } else { dx };
        let (sin, cos) = self.rotation.sin_cos();
        Point2::new(
            self.offset.x + cos * dx - sin * dy,
            self.offset.y + sin * dx + cos * dy,
        )
    }
}

/// The simulated camera's settings.
#[derive(Clone, Debug, PartialEq)]
pub struct SimCameraConfig {
    /// The sensor's width in pixels: a multiple of 12, so that every mode
    /// and every ROI keeps the mosaic's pattern.
    pub width: u32,
    /// The sensor's height in pixels: a multiple of 12.
    pub height: u32,
    /// The sensor's pixel.
    pub pixel_size: Um,
    /// Its electronics.
    pub sensor: SensorModel,
    /// How it sits over the stage.
    pub mounting: Mounting,
    /// How long a frame takes to read out at full resolution, binned 2 × 2
    /// and binned 3 × 3: each row starts a line time, this over the rows,
    /// after the one above.
    pub readout: [Ms; 3],
    /// The seed for the noise.
    pub seed: u64,
    /// The bytes of rendered tiles the camera keeps (`tiles`).
    pub cache: usize,
    /// The optics' vignetting, s: the light falls off as 1 / (1 + s r²)²
    /// with r the distance from the sensor's centre over its half diagonal,
    /// cos⁴'s law. 0 for none (P33).
    pub vignetting: f64,
    /// The sensor's colour response, with the light's tint: the RGB it
    /// reads is this matrix times the light's, by rows. The identity for an
    /// ideal sensor in white light (P33).
    pub response: [[f64; 3]; 3],
}

/// An ideal sensor in white light.
pub const IDEAL: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// The optics' vignetting `s` at a pixel `(dx, dy)` sensor pixels from the
/// sensor's centre, over the half diagonal's square `r2`.
pub fn vignette(s: f64, dx: f64, dy: f64, r2: f64) -> f64 {
    let f = 1.0 + s * (dx * dx + dy * dy) / r2;
    1.0 / (f * f)
}

impl Default for SimCameraConfig {
    /// The MIChrome 20: 5472 × 3648 pixels of 2.4 µm, read out in about
    /// 60 ms at full resolution and 15 ms binned 3 × 3 (P02b), and 18 ms
    /// binned 2 × 2, an estimate from its frame rate.
    fn default() -> Self {
        Self {
            width: 5472,
            height: 3648,
            pixel_size: Um(2.4),
            sensor: SensorModel::default(),
            mounting: Mounting::default(),
            readout: [Ms(60.0), Ms(18.0), Ms(15.0)],
            seed: 1,
            cache: 2 << 30,
            vignetting: 0.0,
            response: IDEAL,
        }
    }
}

impl SimCameraConfig {
    /// The MIChrome 20's camera on a smaller sensor, for tests and quick
    /// examples: its modes, rates and readout times stay the same.
    pub fn small(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            cache: 256 << 20,
            ..Self::default()
        }
    }

    /// The MIChrome 20 scaled down for tests: a quarter of its pixels each
    /// way, 1368 × 912, each four times as large, so it sees the same field.
    pub fn quarter() -> Self {
        Self {
            width: 1368,
            height: 912,
            pixel_size: Um(9.6),
            cache: 512 << 20,
            ..Self::default()
        }
    }

    /// Its capabilities.
    ///
    /// - **Modes** `full`, `bin2` and `bin3`, at the MIChrome 20's measured
    ///   rates for 8 and 16 bits (`docs/hardware.md`), and `design`: full
    ///   resolution in 16 bits at 14.5 fps, the 580 MB/s that P29 records.
    ///   Binned pixels are sums.
    /// - **ROIs** in steps of 12, so that the mosaic stays RGGB in every
    ///   mode, which don't raise the rate.
    /// - **Software and external triggers,** the external one for the
    ///   camera that will replace the MIChrome 20.
    pub fn capabilities(&self) -> CameraCapabilities {
        let line = |k: u32| {
            Some(Ms(
                self.readout[k as usize - 1].0 / f64::from(self.height / k)
            ))
        };
        let mode = |name: &str, k: u32, rates: Vec<ModeRate>| CameraMode {
            name: name.to_owned(),
            width: self.width / k,
            height: self.height / k,
            binning: k,
            rates,
            line_time: line(k),
        };
        let both = |fps8: f64, fps16: f64| {
            vec![
                ModeRate {
                    format: BAYER8,
                    fps: fps8,
                },
                ModeRate {
                    format: BAYER16,
                    fps: fps16,
                },
            ]
        };
        CameraCapabilities {
            model: "simulated MIChrome 20".to_owned(),
            sensor_width: self.width,
            sensor_height: self.height,
            pixel_size: self.pixel_size,
            shutter: Shutter::Rolling,
            modes: vec![
                mode("full", 1, both(14.49, 7.24)),
                mode("bin2", 2, both(53.14, 26.57)),
                mode("bin3", 3, both(67.26, 33.87)),
                mode(
                    "design",
                    1,
                    vec![ModeRate {
                        format: BAYER16,
                        fps: 14.5,
                    }],
                ),
            ],
            exposure_min: Ms(0.0184),
            exposure_max: Ms(19_340.1),
            gain_min: 0.0,
            gain_max: 240.0,
            roi: Some(RoiRules {
                x_step: 12,
                y_step: 12,
                min_size: 48,
                raises_rate: false,
            }),
            trigger: TriggerCapabilities {
                software: true,
                external: true,
                output: false,
            },
            sensor_timestamps: false,
            temperature: false,
        }
    }
}

/// What the camera looks through, and in which light: set by whoever drives
/// the simulator (P16).
#[derive(Clone, Copy, Debug, PartialEq)]
struct View {
    optics: Optics,
    light: f64,
    path: Light,
}

/// What the camera's thread shares with it.
struct Inner {
    config: SimCameraConfig,
    capabilities: CameraCapabilities,
    clock: Arc<SimClock>,
    stage: Arc<SimStage>,
    world: Arc<RwLock<World>>,
    settings: Mutex<CameraSettings>,
    view: Mutex<View>,
    tiles: TileCache,
    /// The GPU's tiles, when frames are exposed there.
    #[cfg(feature = "cuda")]
    gpu: Mutex<Option<crate::gpu::GpuTiles>>,
}

struct Stream {
    stop: Arc<AtomicBool>,
    triggers: Sender<Timestamp>,
    thread: JoinHandle<()>,
}

/// A camera that sees the simulated world through the optics, as the stage
/// truly moves, on the simulation clock (P12).
///
/// - **Each frame** renders the field at the stage's true positions through
///   the [`Mounting`]. Each row is exposed a line time after the one above,
///   for the exposure, so motion shears and blurs the frame.
/// - **The sensor** ([`SensorModel`]) turns the light into counts: a 12-bit
///   ADC, out in 8 bits or aligned high in 16, with noise.
/// - **Frames are paced by the clock:** free-running at the mode's interval,
///   or on a software trigger, or on the stage's trigger pulses.
/// - **If rendering is slower than the frame interval,** frames that ended
///   meanwhile are skipped and counted in the next frame's `dropped_before`.
///   Motion never waits for rendering.
///
/// Acquisition fills in each frame's position, objective and lights
/// (ADR-0012); the camera leaves them empty.
pub struct SimCamera {
    inner: Arc<Inner>,
    stream: Mutex<Option<Stream>>,
    /// Unplugged, a fault injected (P50): it answers nothing.
    unplugged: AtomicBool,
}

impl SimCamera {
    /// A camera on `clock`, over `stage`, looking at `world` through
    /// `optics`, in full transmitted light. It starts in mode `full`, 16
    /// bits, free-running, with a 15 ms exposure and no gain.
    pub fn new(
        config: SimCameraConfig,
        clock: Arc<SimClock>,
        stage: Arc<SimStage>,
        world: Arc<RwLock<World>>,
        optics: Optics,
    ) -> Self {
        let capabilities = config.capabilities();
        let tiles = TileCache::with_response(config.cache, config.response);
        let settings = CameraSettings {
            mode: "full".to_owned(),
            format: BAYER16,
            exposure: Ms(15.0),
            gain: 0.0,
            roi: None,
            trigger: Trigger::FreeRunning,
        };
        Self {
            inner: Arc::new(Inner {
                config,
                capabilities,
                clock,
                stage,
                world,
                settings: Mutex::new(settings),
                tiles,
                #[cfg(feature = "cuda")]
                gpu: Mutex::new(None),
                view: Mutex::new(View {
                    optics,
                    light: 1.0,
                    path: Light::Transmitted,
                }),
            }),
            stream: Mutex::new(None),
            unplugged: AtomicBool::new(false),
        }
    }

    /// Looks through other optics, as when another objective is fitted.
    pub fn set_optics(&self, optics: Optics) {
        lock(&self.inner.view).optics = optics;
    }

    /// Sets the light's intensity, from 0 to 1, and its path.
    pub fn set_light(&self, intensity: f64, path: Light) {
        let mut view = lock(&self.inner.view);
        view.light = intensity.clamp(0.0, 1.0);
        view.path = path;
    }

    /// Exposes frames on `cuda`'s GPU from now on, holding up to `slots`
    /// rendered tiles there: 345 cover a full frame. If the GPU fails, the
    /// camera goes back to the CPU.
    #[cfg(feature = "cuda")]
    pub fn use_gpu(&self, cuda: oscope_compute::cuda::Cuda, slots: usize) -> Result<(), String> {
        let tiles = crate::gpu::GpuTiles::new(cuda, slots).map_err(|e| e.to_string())?;
        *lock(&self.inner.gpu) = Some(tiles);
        Ok(())
    }

    /// Whether frames are exposed on the GPU.
    pub fn on_gpu(&self) -> bool {
        #[cfg(feature = "cuda")]
        {
            lock(&self.inner.gpu).is_some()
        }
        #[cfg(not(feature = "cuda"))]
        {
            false
        }
    }

    /// How its cache of rendered tiles has done.
    pub fn cache_stats(&self) -> CacheStats {
        self.inner.tiles.stats()
    }

    /// The world it looks at, to load or unload slides.
    pub fn world(&self) -> Arc<RwLock<World>> {
        Arc::clone(&self.inner.world)
    }

    /// Unplugs it, or plugs it back in: a fault to inject (P50). Unplugged,
    /// it stops streaming, and refuses everything as disconnected.
    pub fn unplug(&self, unplugged: bool) {
        self.unplugged.store(unplugged, Ordering::Release);
        if unplugged {
            self.halt();
        }
    }

    /// `Disconnected` if it is unplugged.
    fn plugged(&self) -> Result<(), Error> {
        match self.unplugged.load(Ordering::Acquire) {
            true => Err(Error::Disconnected),
            false => Ok(()),
        }
    }

    fn halt(&self) {
        if let Some(stream) = lock(&self.stream).take() {
            stream.stop.store(true, Ordering::Release);
            self.inner.clock.nudge();
            // A panic on the thread has already been reported.
            let _ = stream.thread.join();
        }
    }
}

impl Drop for SimCamera {
    fn drop(&mut self) {
        self.halt();
    }
}

/// One frame's counts, before they are packed.
struct Counts {
    values: Vec<u16>,
    roi: PixelRect,
    width: u32,
    height: u32,
    binning: u32,
    line: Ms,
}

impl Inner {
    /// How long a frame takes from its first row's start to its last row's
    /// end.
    fn span(&self, settings: &CameraSettings) -> Duration {
        let (mode, height) = match self.capabilities.mode(&settings.mode) {
            Ok(m) => (
                m,
                self.capabilities.frame_shape(settings).map_or(0, |s| s.2),
            ),
            Err(_) => return Duration::ZERO,
        };
        let line = mode
            .line_time
            .and_then(|l| l.to_duration())
            .unwrap_or_default();
        let exposure = settings.exposure.to_duration().unwrap_or_default();
        line * height.saturating_sub(1) + exposure
    }

    /// The time between free-running frames: the mode's, or the exposure
    /// if that is longer.
    fn interval(&self, settings: &CameraSettings) -> Duration {
        let fps = self
            .capabilities
            .mode(&settings.mode)
            .ok()
            .and_then(|m| m.fps(settings.format))
            .unwrap_or(1.0);
        let exposure = settings.exposure.to_duration().unwrap_or_default();
        Duration::from_secs_f64(1.0 / fps).max(exposure)
    }

    /// Renders the frame whose first row starts at `start`, at the stage's
    /// true positions, as 12-bit counts.
    fn render(&self, settings: &CameraSettings, start: Timestamp, frame: u64) -> Option<Counts> {
        let _rendering = timing::start("camera.render");
        let mut view = *lock(&self.view);
        // The sensor's pixel is the camera's own, whatever the optics say.
        view.optics.sensor_pixel = self.config.pixel_size;
        let mode = self.capabilities.mode(&settings.mode).ok()?;
        let (roi, width, height) = self.capabilities.frame_shape(settings).ok()?;
        let k = mode.binning.max(1);
        let line = mode.line_time.unwrap_or(Ms(0.0));
        let line_d = line.to_duration()?;
        let exposure = settings.exposure.to_duration()?;
        let at_sample = view.optics.pixel_size().0;
        let p = at_sample * f64::from(k);
        let (cw, ch) = (f64::from(self.config.width), f64::from(self.config.height));
        let mounting = self.config.mounting;
        // The vignetting: an output pixel's centre from the sensor's, in the
        // sensor's pixels, x0 + k u along x, and its half diagonal's square.
        let vignetting = self.config.vignetting;
        let half_diagonal = (cw * cw + ch * ch) / 4.0;
        let (vx0, vy0) = (
            f64::from(roi.x) + 0.5 * f64::from(k) - cw / 2.0,
            f64::from(roi.y) + 0.5 * f64::from(k) - ch / 2.0,
        );
        // An output pixel's centre, from the sensor's centre, in µm at the
        // sample along the sensor's axes, then on the stage.
        let local = |u: f64, v: f64| {
            let dx = (f64::from(roi.x) + (u + 0.5) * f64::from(k) - cw / 2.0) * at_sample;
            let dy = (f64::from(roi.y) + (v + 0.5) * f64::from(k) - ch / 2.0) * at_sample;
            mounting.to_stage(dx, dy)
        };
        let end = start + line_d * height.saturating_sub(1) + exposure;

        // One region of the world, aligned with the stage, covering the
        // sensor's footprint wherever the stage was during the frame.
        let corners = [
            local(-0.5, -0.5),
            local(f64::from(width) - 0.5, -0.5),
            local(-0.5, f64::from(height) - 0.5),
            local(f64::from(width) - 0.5, f64::from(height) - 0.5),
        ];
        let (mut lo, mut hi) = (
            Point2::new(f64::MAX, f64::MAX),
            Point2::new(f64::MIN, f64::MIN),
        );
        let span = end.0.saturating_sub(start.0);
        for j in 0..=32_u64 {
            let at = Timestamp(start.0 + span * j / 32);
            let s = self.stage.true_position(at);
            for c in &corners {
                lo = Point2::new(lo.x.min(s.x.0 + c.x), lo.y.min(s.y.0 + c.y));
                hi = Point2::new(hi.x.max(s.x.0 + c.x), hi.y.max(s.y.0 + c.y));
            }
        }
        let margin = 2.0 * p;
        let region = Rect::new(
            lo.x - margin,
            lo.y - margin,
            hi.x - lo.x + 2.0 * margin,
            hi.y - lo.y + 2.0 * margin,
        );
        let middle = start + line_d * (height / 2) + exposure / 2;
        let optics = Optics {
            sensor_pixel: view.optics.sensor_pixel * f64::from(k),
            ..view.optics
        };
        let world = self
            .world
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        // The rendered tiles, from the cache: only those it lacks are
        // rendered now.
        let z = self.stage.true_position(middle).z;
        let field = timing::time("camera.field", || {
            self.tiles.field(&world, &optics, view.path, z, region)
        });
        let sample = |x: f64, y: f64, c: usize| field.sample(x, y, c);

        // Where the stage is during each row's exposure: enough places to
        // blur its motion a pixel at a time, up to 64.
        let motion = timing::start("camera.motion");
        let rows: Vec<Vec<Point2>> = (0..height)
            .into_par_iter()
            .map(|v| {
                let first = start + line_d * v;
                let (a, b) = (
                    self.stage.true_position(first),
                    self.stage.true_position(first + exposure),
                );
                let moved = ((b.x.0 - a.x.0).powi(2) + (b.y.0 - a.y.0).powi(2)).sqrt() / p;
                let n = (moved.ceil() as u64 + 1).clamp(1, 64);
                (0..n)
                    .map(|j| {
                        let at = first + exposure.mul_f64((j as f64 + 0.5) / n as f64);
                        let s = self.stage.true_position(at);
                        Point2::new(s.x.0, s.y.0)
                    })
                    .collect()
            })
            .collect();
        motion.stop();
        let sensor = self.config.sensor;
        let exposure_ms = settings.exposure.0;
        let scale = view.light * sensor.electrons_per_ms * exposure_ms * f64::from(k * k);
        let counts = |values| Counts {
            values,
            roi,
            width,
            height,
            binning: k,
            line,
        };

        #[cfg(feature = "cuda")]
        {
            let mut gpu = lock(&self.gpu);
            if let Some(device) = gpu.as_mut() {
                let affine = {
                    let (o, x, y) = (local(0.0, 0.0), local(1.0, 0.0), local(0.0, 1.0));
                    [x.x - o.x, y.x - o.x, o.x, x.y - o.y, y.y - o.y, o.y]
                };
                let job = GpuFrame {
                    field: &field,
                    rows: &rows,
                    affine,
                    size: (width, height),
                    scale,
                    exposure_s: exposure_ms / 1e3,
                    pixels: k * k,
                    gain: settings.gain,
                    sensor,
                    seed: self.config.seed,
                    frame,
                    vignette: [vx0, vy0, f64::from(k), half_diagonal, vignetting],
                };
                match timing::time("camera.expose_gpu", || device.expose(&job)) {
                    Ok(values) => return Some(counts(values)),
                    // The GPU failed: this frame and the rest on the CPU.
                    Err(_) => *gpu = None,
                }
            }
        }

        // Each row, over its own exposure; then the sensor.
        let exposing = timing::start("camera.expose");
        let mut values = vec![0_u16; width as usize * height as usize];
        values
            .par_chunks_mut(width as usize)
            .enumerate()
            .for_each(|(v, row)| {
                let places = &rows[v];
                let n = places.len();
                let mut random = Random::new(hash(self.config.seed, frame, v as i64, 0));
                for (u, out) in row.iter_mut().enumerate() {
                    let here = local(u as f64, v as f64);
                    // RGGB: red, green / green, blue.
                    let c = [[0, 1], [1, 2]][v % 2][u % 2];
                    let light = places
                        .iter()
                        .map(|s| sample(s.x + here.x, s.y + here.y, c))
                        .sum::<f64>()
                        / n as f64;
                    let (dx, dy) = (vx0 + f64::from(k) * u as f64, vy0 + f64::from(k) * v as f64);
                    let falloff = vignette(vignetting, dx, dy, half_diagonal);
                    *out = sensor.count(
                        light * scale * falloff,
                        exposure_ms / 1e3,
                        k * k,
                        settings.gain,
                        &mut random,
                    );
                }
            });
        exposing.stop();
        Some(counts(values))
    }

    /// Streams from `begins` until `stop`: the camera's thread.
    fn run(
        &self,
        pool: FramePool,
        sink: FrameSink,
        stop: &AtomicBool,
        triggers: Receiver<Timestamp>,
        begins: Timestamp,
    ) {
        let first = lock(&self.settings).clone();
        let (mut id, mut dropped) = (FrameId(0), 0_u64);
        let (mut next, mut last_pulse) = (begins, begins);
        while !stop.load(Ordering::Acquire) {
            let settings = lock(&self.settings).clone();
            let span = self.span(&settings);
            // When the next exposure starts.
            let start = match first.trigger {
                Trigger::FreeRunning => next,
                Trigger::Software => match triggers.recv_timeout(Duration::from_millis(5)) {
                    Ok(at) => at,
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => return,
                },
                Trigger::External { .. } => {
                    let pulse = self
                        .stage
                        .pulses()
                        .into_iter()
                        .find(|p| p.time > last_pulse);
                    let Some(pulse) = pulse else {
                        let soon = self.clock.now() + Duration::from_millis(1);
                        if !self.clock.block_until(soon, stop) {
                            return;
                        }
                        continue;
                    };
                    last_pulse = pulse.time;
                    pulse.time
                }
            };
            // The frame exists once its last row's exposure has ended.
            if !self.clock.block_until(start + span, stop) {
                return;
            }
            // How late the camera starts on the frame, after its exposure.
            let late = self.clock.now().saturating_since(start + span);
            timing::record("camera.wake_late", late);
            if let Some(counts) = self.render(&settings, start, id.0) {
                match pool.take() {
                    None => dropped += 1,
                    Some(mut buffer) => {
                        let packing = timing::start("camera.pack");
                        let bytes = buffer.bytes_mut();
                        if first.format == BAYER8 {
                            for (b, c) in bytes.iter_mut().zip(&counts.values) {
                                *b = (c >> 4) as u8;
                            }
                        } else {
                            for (b, c) in
                                bytes.as_chunks_mut::<2>().0.iter_mut().zip(&counts.values)
                            {
                                *b = (c << 4).to_le_bytes();
                            }
                        }
                        packing.stop();
                        let mut meta = FrameMeta::new(
                            id,
                            first.format,
                            counts.width,
                            counts.height,
                            self.clock.now(),
                        );
                        meta.roi = counts.roi;
                        meta.binning = counts.binning;
                        meta.mode = settings.mode.clone();
                        meta.exposure_start = start;
                        meta.exposure = settings.exposure;
                        meta.line_time = Some(counts.line);
                        meta.gain = settings.gain;
                        let black = f64::from(self.config.sensor.black);
                        meta.black_level = if first.format == BAYER8 {
                            black / 16.0
                        } else {
                            black * 16.0
                        };
                        meta.dropped_before = dropped;
                        dropped = 0;
                        sink(buffer, meta);
                        id = id.next();
                    }
                }
            }
            if first.trigger == Trigger::FreeRunning {
                // On to the next exposure; any that ended while this one
                // rendered are skipped, and counted.
                let interval = self.interval(&settings);
                next = start + interval;
                let now = self.clock.now();
                while next + interval + span <= now {
                    next = next + interval;
                    dropped += 1;
                }
            }
        }
    }
}

impl Camera for SimCamera {
    fn capabilities(&self) -> &CameraCapabilities {
        &self.inner.capabilities
    }

    fn settings(&self) -> HalFuture<'_, CameraSettings> {
        Box::pin(async move {
            self.plugged()?;
            Ok(lock(&self.inner.settings).clone())
        })
    }

    fn configure(&self, settings: CameraSettings) -> HalFuture<'_, CameraSettings> {
        Box::pin(async move {
            self.plugged()?;
            let applied = self.inner.capabilities.check(&settings)?;
            let streaming = self.streaming();
            let mut current = lock(&self.inner.settings);
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
            self.plugged()?;
            let mut stream = lock(&self.stream);
            if stream.is_some() {
                return Err(Error::Busy {
                    reason: "it is already streaming".to_owned(),
                });
            }
            let settings = lock(&self.inner.settings).clone();
            let (_, width, height) = self.inner.capabilities.frame_shape(&settings)?;
            let needed = settings.format.frame_bytes(width, height);
            if pool.stats().slab_len < needed {
                return Err(Error::OutOfRange {
                    what: "the pool's slab size in bytes".to_owned(),
                    value: pool.stats().slab_len as f64,
                    min: needed as f64,
                    max: f64::INFINITY,
                });
            }
            // The stream starts now, whenever its thread first runs.
            let begins = self.inner.clock.now();
            let stop = Arc::new(AtomicBool::new(false));
            let (triggers, received) = mpsc::channel();
            let (inner, flag) = (Arc::clone(&self.inner), Arc::clone(&stop));
            let thread = thread::Builder::new()
                .name("simulated camera".to_owned())
                .spawn(move || inner.run(pool, sink, &flag, received, begins))
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
            self.halt();
            Ok(())
        })
    }

    fn streaming(&self) -> bool {
        lock(&self.stream).is_some()
    }

    fn trigger(&self) -> HalFuture<'_, ()> {
        Box::pin(async move {
            let stream = lock(&self.stream);
            let busy = |reason: &str| Error::Busy {
                reason: reason.to_owned(),
            };
            let stream = stream.as_ref().ok_or_else(|| busy("it isn't streaming"))?;
            if lock(&self.inner.settings).trigger != Trigger::Software {
                return Err(busy("it isn't set to a software trigger"));
            }
            let now = self.inner.clock.now();
            stream.triggers.send(now).map_err(|_| Error::Disconnected)
        })
    }
}
