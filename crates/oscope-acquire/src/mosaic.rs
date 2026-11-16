//! The live mosaic (ADR-0026; plan §5.15): frames drawn on a canvas as they
//! arrive, each at its exposure midpoint's position through the calibrated
//! affine.
//!
//! - **Free roaming:** the live stream's newest frame, drawn if the stage
//!   moved under a canvas pixel during its exposure, and if it shows a new
//!   place, or a place drawn but sharper.
//! - **Scans:** the frames a scan takes, each drawn as it comes.
//! - **Drawn as the preview draws:** in the same look and live recipe, at
//!   about the canvas's pixel.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use oscope_compute::{ComputeError, Image, Shape, focus};
use oscope_core::{Frame, FrameMeta, Xyz};
use oscope_hal::PositionTrack;
use oscope_imaging::color::{self, ColorError};
use oscope_stitch::{Affine, Blend, Canvas, CanvasError, CanvasGeometry, Changes, Rgb, TILE};
use oscope_stitch::{TileKey, canvas::TILE_BYTES};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::{Notify, mpsc};

use crate::bus::LatestReceiver;
use crate::preview::{Displayed, Look, Preview, PreviewError, PreviewRequest};

/// How many of a scan's frames may wait to be drawn.
const SCAN_QUEUE: usize = 8;
/// How many frames drawn are remembered, for free roaming's gates.
const RECENT: usize = 256;
/// A tile's JPEG quality.
const QUALITY: u8 = 85;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What a live mosaic can't do.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MosaicError {
    /// Its setup can't be.
    #[error("{0}")]
    Setup(String),
    /// Its canvas failed.
    #[error(transparent)]
    Canvas(#[from] CanvasError),
    /// A frame couldn't be drawn as the preview draws it.
    #[error(transparent)]
    Preview(#[from] PreviewError),
    /// A frame's sharpness couldn't be measured.
    #[error(transparent)]
    Compute(#[from] ComputeError),
    /// A tile couldn't be encoded.
    #[error(transparent)]
    Image(#[from] ColorError),
}

/// How free roaming chooses its frames, each with a default.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Gates {
    /// The most the stage may move during a frame's exposure, in canvas
    /// pixels: 1.
    pub blur_px: f64,
    /// How near a frame drawn, as a part of the frame's shorter side, a
    /// frame shows the same place: 0.25.
    pub near: f64,
    /// How many times sharper than the sharpest drawn at its place a frame
    /// must be to be drawn over it: 1.1.
    pub sharper: f64,
}

impl Default for Gates {
    fn default() -> Self {
        Self {
            blur_px: 1.0,
            near: 0.25,
            sharper: 1.1,
        }
    }
}

impl Gates {
    /// Checks them: a blur over 0, near from 0 to 1, and sharper at least 1.
    pub fn check(&self) -> Result<(), MosaicError> {
        let ok = self.blur_px > 0.0
            && self.blur_px.is_finite()
            && self.near > 0.0
            && self.near <= 1.0
            && self.sharper >= 1.0
            && self.sharper.is_finite();
        match ok {
            true => Ok(()),
            false => Err(MosaicError::Setup(format!(
                "gates of {self:?}: a blur over 0 px, near from 0 to 1, and sharper at least 1 \
                 are needed"
            ))),
        }
    }
}

/// What a live mosaic draws, and on what.
#[derive(Clone, Debug)]
pub struct MosaicSetup {
    /// The canvas.
    pub geometry: CanvasGeometry,
    /// How frames are drawn over what is there.
    pub blend: Blend,
    /// The objective its frames come through: frames through another
    /// aren't drawn.
    pub objective: Option<String>,
    /// The camera's binning: frames at another aren't drawn.
    pub binning: u32,
    /// M at that binning, in µm per pixel, by rows (P32): the stage move
    /// `M · d` brings the content `d` pixels from the sensor's centre to
    /// its centre.
    pub matrix: [[f64; 2]; 2],
    /// The sensor's width and height, in its own pixels: where the stage's
    /// position looks is its centre.
    pub sensor: (u32, u32),
    /// Free roaming's gates.
    pub gates: Gates,
    /// The most bytes of tiles kept in memory.
    pub budget: usize,
    /// Where the tiles past the budget go.
    pub folder: std::path::PathBuf,
}

/// What became of a frame offered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// It was drawn.
    Drawn,
    /// The stage moved too far during its exposure.
    Blurred,
    /// It shows a place drawn, and isn't sharper.
    Same,
    /// It has no position, or the track doesn't reach its exposure.
    Unplaced,
    /// It came through another objective, or at another binning.
    Other,
    /// It lies off the canvas.
    Outside,
    /// It failed to draw.
    Failed,
}

/// How many frames became what.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MosaicCounts {
    /// Drawn.
    pub drawn: u64,
    /// Of those drawn, a scan's.
    pub scans: u64,
    /// Blurred by the stage's moving.
    pub blurred: u64,
    /// Showing a place drawn, and no sharper.
    pub same: u64,
    /// Without a position.
    pub unplaced: u64,
    /// Through another objective, or at another binning.
    pub other: u64,
    /// Off the canvas.
    pub outside: u64,
    /// That failed to draw.
    pub failed: u64,
    /// A scan's frames not drawn, as too many were waiting.
    pub dropped: u64,
}

/// The look frames are drawn in, as the preview has it now, and whether
/// to draw on the GPU.
pub type LookSource = Arc<dyn Fn() -> (Look, bool) + Send + Sync>;

/// Where a frame drawn showed, in canvas pixels, and how sharp it was.
struct Drawn {
    at: (f64, f64),
    sharpness: f64,
}

/// A live mosaic. See [the module](self).
pub struct LiveMosaic {
    setup: MosaicSetup,
    canvas: Mutex<Canvas>,
    drawn: Mutex<VecDeque<Drawn>>,
    counts: Mutex<MosaicCounts>,
    /// Where the last frame roaming judged was, and what became of it.
    last: Mutex<Option<(Xyz, Verdict)>>,
    preview: Preview,
    look: LookSource,
    roaming: bool,
    stopped: AtomicBool,
    halt: Arc<Notify>,
    scans: mpsc::Sender<(Frame, Xyz)>,
}

impl LiveMosaic {
    /// Starts a live mosaic on `setup`'s canvas, drawing as `preview` draws
    /// in `look`. It draws the frames handed to [`scan_frame`](Self::scan_frame)
    /// and, given `roam`, the newest of the live stream's, gated by the
    /// stage's track. Call within a Tokio runtime.
    pub fn start(
        setup: MosaicSetup,
        preview: Preview,
        look: LookSource,
        roam: Option<(LatestReceiver, Arc<PositionTrack>)>,
    ) -> Result<Arc<Self>, MosaicError> {
        setup.gates.check()?;
        let [[a, b], [c, d]] = setup.matrix;
        let det = a * d - b * c;
        if setup.binning == 0 || setup.sensor.0 == 0 || setup.sensor.1 == 0 || !det.is_normal() {
            return Err(MosaicError::Setup(format!(
                "a binning of {}, a sensor of {:?} and a matrix of {:?}: a binning and a \
                 sensor over 0, and a matrix that can be inverted, are needed",
                setup.binning, setup.sensor, setup.matrix
            )));
        }
        let canvas = Canvas::new(setup.geometry, setup.budget, &setup.folder)?;
        let (scans, mut queue) = mpsc::channel(SCAN_QUEUE);
        let halt = Arc::new(Notify::new());
        let mosaic = Arc::new(Self {
            setup,
            canvas: Mutex::new(canvas),
            drawn: Mutex::new(VecDeque::new()),
            counts: Mutex::new(MosaicCounts::default()),
            last: Mutex::new(None),
            preview,
            look,
            roaming: roam.is_some(),
            stopped: AtomicBool::new(false),
            halt: Arc::clone(&halt),
            scans,
        });
        // A scan's frames, in turn, until the mosaic goes.
        let drawer = Arc::downgrade(&mosaic);
        tokio::spawn(async move {
            while let Some((frame, at)) = queue.recv().await {
                let Some(mosaic) = drawer.upgrade() else {
                    break;
                };
                let drawn = tokio::task::spawn_blocking(move || mosaic.offer(&frame, Some(at)));
                if drawn.await.is_err() {
                    break;
                }
            }
        });
        // The live stream's newest frame, gated, until stopped.
        if let Some((mut frames, track)) = roam {
            let roamer = Arc::downgrade(&mosaic);
            tokio::spawn(async move {
                loop {
                    // A halt first: one asked while a frame was drawn waits.
                    let frame = tokio::select! {
                        biased;
                        () = halt.notified() => None,
                        frame = frames.recv() => frame,
                    };
                    let (Some(frame), Some(mosaic)) = (frame, roamer.upgrade()) else {
                        break;
                    };
                    let track = Arc::clone(&track);
                    let drawn = tokio::task::spawn_blocking(move || {
                        mosaic.roam(&frame, &track);
                    });
                    if drawn.await.is_err() {
                        break;
                    }
                }
            });
        }
        Ok(mosaic)
    }

    /// Its canvas's geometry.
    pub fn geometry(&self) -> CanvasGeometry {
        self.setup.geometry
    }

    /// How it draws frames over what is there.
    pub fn blend(&self) -> Blend {
        self.setup.blend
    }

    /// Whether it draws the live stream's frames, while not stopped.
    pub fn roaming(&self) -> bool {
        self.roaming
    }

    /// How many frames became what.
    pub fn counts(&self) -> MosaicCounts {
        *lock(&self.counts)
    }

    /// Its tiles in memory and in all, and their bytes in memory.
    pub fn tiles(&self) -> (usize, usize) {
        let canvas = lock(&self.canvas);
        (canvas.resident_bytes() / TILE_BYTES, canvas.tile_count())
    }

    /// The tiles changed since `generation`.
    pub fn changes_since(&self, generation: u64) -> Changes {
        lock(&self.canvas).changes_since(generation)
    }

    /// Whether it has stopped drawing.
    pub fn stopped(&self) -> bool {
        self.stopped.load(Ordering::Relaxed)
    }

    /// Stops drawing new frames. The canvas stays to be seen.
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Relaxed);
        self.halt.notify_one();
    }

    /// Empties the canvas.
    pub fn clear(&self) -> Result<(), MosaicError> {
        lock(&self.drawn).clear();
        *lock(&self.last) = None;
        Ok(lock(&self.canvas).clear()?)
    }

    /// Hands it a frame a scan took at `at`, to draw as it can: whether
    /// it took it. One that would wait behind too many is counted as
    /// dropped; a stopped mosaic takes none.
    pub fn scan_frame(&self, frame: Frame, at: Xyz) -> bool {
        if self.stopped() {
            return false;
        }
        let taken = self.scans.try_send((frame, at)).is_ok();
        if !taken {
            lock(&self.counts).dropped += 1;
        }
        taken
    }

    /// Tile `key`'s RGBA, as [`Canvas::tile`].
    pub fn tile(&self, key: TileKey) -> Result<Option<Vec<u8>>, MosaicError> {
        Ok(lock(&self.canvas).tile(key)?)
    }

    /// Tile `key` as a JPEG, its bare pixels black and cut at its level's
    /// edge: none if nothing was drawn on it.
    pub fn tile_jpeg(&self, key: TileKey) -> Result<Option<Vec<u8>>, MosaicError> {
        let geometry = self.setup.geometry;
        if key.level >= geometry.levels() {
            return Ok(None);
        }
        let Some(rgba) = lock(&self.canvas).tile(key)? else {
            return Ok(None);
        };
        let (width, height) = geometry.size(key.level);
        let w = (width - (key.col * TILE).min(width)).min(TILE);
        let h = (height - (key.row * TILE).min(height)).min(TILE);
        if w == 0 || h == 0 {
            return Ok(None);
        }
        // Bare pixels are black: nothing was drawn there. Above level 0, a
        // pixel partly covered has the colour of what covers it.
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for row in rgba
            .as_chunks::<{ TILE as usize * 4 }>()
            .0
            .iter()
            .take(h as usize)
        {
            for &[r, g, b, _] in row.as_chunks::<4>().0.iter().take(w as usize) {
                rgb.extend([r, g, b]);
            }
        }
        Ok(Some(color::encode_jpeg(&rgb, w, h, QUALITY)?))
    }

    /// Offers a scan's frame, taken at `at` if it has no position of its
    /// own: drawn ungated.
    pub fn offer(&self, frame: &Frame, at: Option<Xyz>) -> Verdict {
        let verdict = self.count(self.draw(frame, at, None));
        if verdict == Verdict::Drawn {
            lock(&self.counts).scans += 1;
        }
        verdict
    }

    /// Offers a frame of the live stream, gated by the stage's `track`.
    /// While the stage stays where a frame was judged to show a place
    /// drawn, within a quarter of a canvas pixel, each frame is judged the
    /// same, undrawn and unmeasured.
    pub fn roam(&self, frame: &Frame, track: &PositionTrack) -> Verdict {
        let at = frame.meta().position;
        let still = self.setup.geometry.pixel_um / 4.0;
        if let (Some(at), Some((was, Verdict::Same))) = (at, *lock(&self.last)) {
            let (dx, dy, dz) = (at.x.0 - was.x.0, at.y.0 - was.y.0, at.z.0 - was.z.0);
            if dx.hypot(dy).hypot(dz) < still {
                return self.count(Ok(Verdict::Same));
            }
        }
        let verdict = self.count(self.draw(frame, None, Some(track)));
        *lock(&self.last) = at.map(|at| (at, verdict));
        verdict
    }

    fn count(&self, verdict: Result<Verdict, MosaicError>) -> Verdict {
        let verdict = verdict.unwrap_or_else(|error| {
            tracing::warn!("the live mosaic: {error}");
            Verdict::Failed
        });
        let mut counts = lock(&self.counts);
        *match verdict {
            Verdict::Drawn => &mut counts.drawn,
            Verdict::Blurred => &mut counts.blurred,
            Verdict::Same => &mut counts.same,
            Verdict::Unplaced => &mut counts.unplaced,
            Verdict::Other => &mut counts.other,
            Verdict::Outside => &mut counts.outside,
            Verdict::Failed => &mut counts.failed,
        } += 1;
        verdict
    }

    fn draw(
        &self,
        frame: &Frame,
        at: Option<Xyz>,
        track: Option<&PositionTrack>,
    ) -> Result<Verdict, MosaicError> {
        let (meta, setup) = (frame.meta(), &self.setup);
        if meta.binning != setup.binning || meta.objective != setup.objective {
            return Ok(Verdict::Other);
        }
        let Some(position) = meta.position.or(at) else {
            return Ok(Verdict::Unplaced);
        };
        let pixel_um = setup.geometry.pixel_um;
        // The stage's velocity, in µm a ms, from the exposure's start to its
        // midpoint, as far as the track reaches: none for a scan's frames.
        let mut velocity = (0.0, 0.0);
        if let Some(track) = track {
            let (start, middle) = (meta.exposure_start, meta.exposure_midpoint());
            let (Some(a), Some(b)) = (track.at(start), track.at(middle)) else {
                return Ok(Verdict::Unplaced);
            };
            let ms = middle.saturating_since(start).as_secs_f64() * 1e3;
            if ms > 0.0 {
                velocity = ((b.x.0 - a.x.0) / ms, (b.y.0 - a.y.0) / ms);
            }
            // Blurred: how far it moved while a row was exposed.
            let blur = velocity.0.hypot(velocity.1) * meta.exposure.0 / pixel_um;
            if blur > setup.gates.blur_px {
                return Ok(Verdict::Blurred);
            }
        }
        // Drawn as the preview draws, at about the canvas's pixel.
        let [[a, b], [c, d]] = setup.matrix;
        let shrink = (pixel_um / (a * d - b * c).abs().sqrt()).max(1.0);
        let side = |n: u32| ((f64::from(n) / shrink).round() as u32).max(2);
        let request = PreviewRequest {
            viewport: (side(meta.width), side(meta.height)),
            roi: None,
            quality: QUALITY,
        };
        let (look, gpu) = (self.look)();
        let shown = self.preview.display(frame, &request, &look, gpu)?;
        let to_canvas = self.to_canvas(position, meta, &shown, velocity);
        let (w, h) = (f64::from(shown.width), f64::from(shown.height));
        let centre = to_canvas.apply(w / 2.0, h / 2.0);
        let sharpness = sharpness(&shown)?;
        if track.is_some() {
            let near = setup.gates.near * w.min(h) * to_canvas.scale();
            let best = lock(&self.drawn)
                .iter()
                .filter(|d| (d.at.0 - centre.0).hypot(d.at.1 - centre.1) <= near)
                .map(|d| d.sharpness)
                .fold(None, |best: Option<f64>, s| {
                    Some(best.map_or(s, |b| b.max(s)))
                });
            if best.is_some_and(|best| sharpness < setup.gates.sharper * best) {
                return Ok(Verdict::Same);
            }
        }
        let image = Rgb {
            pixels: &shown.rgb,
            width: shown.width,
            height: shown.height,
        };
        let changed = lock(&self.canvas).draw(image, &to_canvas, setup.blend)?;
        if changed.is_empty() {
            return Ok(Verdict::Outside);
        }
        let mut drawn = lock(&self.drawn);
        if drawn.len() == RECENT {
            drawn.pop_front();
        }
        drawn.push_back(Drawn {
            at: centre,
            sharpness,
        });
        Ok(Verdict::Drawn)
    }

    /// The map from `shown`'s pixels to the canvas's, for a frame taken
    /// with the stage at `at` at its exposure's midpoint, moving at
    /// `velocity`: through the sensor's pixels it shows, their offset from
    /// the sensor's centre at the binning, M, and the canvas's corner and
    /// pixel.
    ///
    /// A rolling shutter exposes row `r` a line time × `(r − h / 2)` after
    /// the middle row: moving steadily, that row shows where the stage was
    /// then, which the map also takes in, as it is linear in the row.
    fn to_canvas(
        &self,
        at: Xyz,
        meta: &FrameMeta,
        shown: &Displayed,
        velocity: (f64, f64),
    ) -> Affine {
        let setup = &self.setup;
        let [[a, b], [c, d]] = setup.matrix;
        let (bin, q) = (f64::from(setup.binning), setup.geometry.pixel_um);
        let roi = shown.roi;
        // Binned pixels a pixel shown spans, and where the first lies from
        // the sensor's centre.
        let kx = f64::from(roi.width) / f64::from(shown.width) / bin;
        let ky = f64::from(roi.height) / f64::from(shown.height) / bin;
        let ox = (f64::from(roi.x) - f64::from(setup.sensor.0) / 2.0) / bin;
        let oy = (f64::from(roi.y) - f64::from(setup.sensor.1) / 2.0) / bin;
        // The stage's move a row, in µm, and the first row shown's from the
        // middle row.
        let line = meta.line_time.map_or(0.0, |line| line.0);
        let (sx, sy) = (velocity.0 * line, velocity.1 * line);
        let first = (f64::from(roi.y) - f64::from(meta.roi.y)) / bin - f64::from(meta.height / 2);
        let (x0, y0) = (setup.geometry.x_um, setup.geometry.y_um);
        Affine([
            [
                a * kx / q,
                (b + sx) * ky / q,
                (at.x.0 - x0 + a * ox + b * oy + sx * first) / q,
            ],
            [
                c * kx / q,
                (d + sy) * ky / q,
                (at.y.0 - y0 + c * ox + d * oy + sy * first) / q,
            ],
        ])
    }
}

impl Drop for LiveMosaic {
    fn drop(&mut self) {
        self.halt.notify_one();
    }
}

/// Brenner's gradient of `shown`'s luminance, over its mean squared (P34).
fn sharpness(shown: &Displayed) -> Result<f64, MosaicError> {
    let grey = shown
        .rgb
        .as_chunks::<3>()
        .0
        .iter()
        .map(|&[r, g, b]| 0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b));
    let shape = Shape::new(shown.width, shown.height, 1);
    let image = Image::from_vec(shape, grey.collect())?;
    Ok(focus::measures(&image)?.brenner)
}
