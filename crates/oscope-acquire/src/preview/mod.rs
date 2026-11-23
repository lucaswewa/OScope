//! The preview (plan §5.5, D6): a raw frame made into a JPEG the size of a
//! client's viewport, in this order:
//!
//! 1. **the ROI:** the sensor's pixels the client asks for, mapped to the
//!    frame's, and widened to whole cells of the mosaic;
//! 2. **demosaicing:** a 2 × 2 superpixel when the output is at most half
//!    the ROI's size, which bins as it goes, and bilinear otherwise;
//! 3. **binning** by the largest whole factor that keeps the output at
//!    least the size asked for;
//! 4. **the black level, white balance and the display transform,** to 8
//!    bits;
//! 5. **resizing** to the size asked for, with a box filter over the
//!    fraction left;
//! 6. **JPEG,** OScope's own encoder.
//!
//! Steps 1–4 run on the GPU when there is one, through the colour steps'
//! CUDA twins, which give the same result to the bit, and 5–6 on the CPU.
//! All of it runs in the live pool, so batch work can't hold it up.
//!
//! A [`LiveRecipe`] (P22), when set, runs after step 4's black level, in
//! place of white balance and the display transform: it gives display
//! values.
//!
//! [`Adaptation`] lowers a client's quality, then its size, while it keeps
//! dropping messages. [`wire`] is how a preview travels (ADR-0014), and
//! [`publish`] writes previews to the desktop app's shared-memory ring
//! (ADR-0015).

mod adapt;
mod recipe;
mod resize;
mod ring;
pub mod wire;

pub use adapt::Adaptation;
pub use recipe::LiveRecipe;
pub use ring::{RingSettings, publish};

use std::sync::{Arc, PoisonError, RwLock};
use std::time::{Duration, Instant};

use oscope_compute::{Compute, ComputeError, Image, Lane, Shape, cpu};
use oscope_core::{BayerPattern, Frame, FrameMeta, FrameView, PixelFormat, PixelRect};
use oscope_imaging::color::{self, ColorError, ColourCorrection, Demosaic, Display};
use oscope_pipeline::Value;
use oscope_pipeline::stock::Region;
use rayon::prelude::*;

use crate::stats::{self, LiveStats, Mask, StatsRequest};

/// What a client asks for.
#[derive(Clone, Debug, PartialEq)]
pub struct PreviewRequest {
    /// The viewport's width and height in pixels. The ROI is fitted inside,
    /// keeping its shape, and never enlarged: the client scales up.
    pub viewport: (u32, u32),
    /// The sensor's pixels to show, in full-sensor coordinates (ADR-0007):
    /// the whole frame if `None`.
    pub roi: Option<PixelRect>,
    /// The JPEG quality, from 1 to 100.
    pub quality: u8,
}

/// How previews look.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Look {
    /// The levels and the tone curve, once the frame's black level is
    /// taken off.
    pub display: Display,
    /// White balance and the colour matrix, for colour frames.
    pub correction: ColourCorrection,
}

/// A preview.
#[derive(Clone, Debug)]
pub struct Rendered {
    /// The JPEG.
    pub jpeg: Vec<u8>,
    /// The sensor's pixels it shows, in full-sensor coordinates: the ROI
    /// asked for, clipped to the frame and widened to whole cells.
    pub roi: PixelRect,
    /// Its width in pixels.
    pub width: u32,
    /// Its height.
    pub height: u32,
    /// Whether the GPU made it.
    pub gpu: bool,
    /// How long it took.
    pub took: Duration,
    /// How long the live recipe took, if one ran.
    pub recipe: Option<Duration>,
}

/// A preview before its JPEG.
#[derive(Clone, Debug)]
pub struct Displayed {
    /// Its 8-bit RGB, by rows.
    pub rgb: Vec<u8>,
    /// The sensor's pixels it shows, as [`Rendered::roi`].
    pub roi: PixelRect,
    /// Its width in pixels.
    pub width: u32,
    /// Its height.
    pub height: u32,
    /// Whether the GPU made it.
    pub gpu: bool,
    /// How long the live recipe took, if one ran.
    pub recipe: Option<Duration>,
}

/// What can't be previewed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum PreviewError {
    /// The frame isn't a mosaic, grey or RGB, in one plane.
    #[error("{0:?} frames can't be previewed")]
    Format(PixelFormat),
    /// The ROI asked for is empty, or misses the frame.
    #[error("the ROI {roi:?} misses the frame's {frame:?}")]
    Roi {
        /// The ROI.
        roi: PixelRect,
        /// The sensor's pixels the frame holds.
        frame: PixelRect,
    },
    /// A colour step failed.
    #[error(transparent)]
    Color(#[from] ColorError),
    /// The live recipe failed, or gave what it mustn't.
    #[error("the live recipe: {0}")]
    Recipe(String),
}

impl From<ComputeError> for PreviewError {
    fn from(error: ComputeError) -> Self {
        Self::Color(error.into())
    }
}

/// The steps of the chain for one preview.
struct Steps {
    pattern: Option<BayerPattern>,
    method: Demosaic,
    bin: u32,
    black: f32,
    look: Look,
}

/// Makes previews in the live pool, on the GPU when there is one. Its
/// clones share its live recipe.
#[derive(Clone, Debug)]
pub struct Preview {
    compute: Arc<Compute>,
    recipe: Arc<RwLock<Option<LiveRecipe>>>,
}

/// What steps 2–4 made: the 8-bit samples, their shape, whether the GPU
/// made them, and how long the live recipe took, if one ran.
type Coloured = (Vec<u8>, Shape, bool, Option<Duration>);

impl Preview {
    /// Previews made with `compute`.
    pub fn new(compute: Arc<Compute>) -> Self {
        let recipe = Arc::default();
        Self { compute, recipe }
    }

    /// The compute layer it uses.
    pub fn compute(&self) -> &Arc<Compute> {
        &self.compute
    }

    /// Sets the live recipe, for every preview from now on, or none.
    pub fn set_recipe(&self, recipe: Option<LiveRecipe>) {
        *self.recipe.write().unwrap_or_else(PoisonError::into_inner) = recipe;
    }

    /// The live recipe, if one is set.
    pub fn recipe(&self) -> Option<LiveRecipe> {
        self.recipe
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The whole frame in linear light, from 0 to 1, as the chain makes it
    /// before its look: demosaiced, bilinear, with the black level taken
    /// off. For references, such as flat fields, in the batch pool.
    pub fn linear(&self, frame: &Frame) -> Result<Image, PreviewError> {
        let (meta, view) = (frame.meta(), frame.view());
        let format = view.format();
        if !cpu::converts(format) || !matches!(format.channels(), 1 | 3) {
            return Err(PreviewError::Format(meta.format));
        }
        self.compute.pools().install(Lane::Batch, || {
            let mut image = cpu::to_f32(&view)?;
            if let Some(pattern) = format.bayer() {
                image = color::demosaic(&image, pattern, Demosaic::Bilinear)?;
            }
            if black(meta) > 0.0 {
                color::subtract_black(&mut image, black(meta))?;
            }
            Ok(image)
        })
    }

    /// Whether there is a GPU to make previews on.
    pub fn has_gpu(&self) -> bool {
        #[cfg(feature = "cuda")]
        return self.compute.cuda().is_some();
        #[cfg(not(feature = "cuda"))]
        false
    }

    /// `frame` as `request` asks, looking as `look` says: on the GPU if
    /// `gpu` and there is one.
    pub fn render(
        &self,
        frame: &Frame,
        request: &PreviewRequest,
        look: &Look,
        gpu: bool,
    ) -> Result<Rendered, PreviewError> {
        let started = Instant::now();
        let shown = self.display(frame, request, look, gpu)?;
        let (rgb, width, height) = (&shown.rgb, shown.width, shown.height);
        let jpeg = self.compute.pools().install(Lane::Live, || {
            color::encode_jpeg(rgb, width, height, request.quality)
        })?;
        Ok(Rendered {
            jpeg,
            roi: shown.roi,
            width,
            height,
            gpu: shown.gpu,
            took: started.elapsed(),
            recipe: shown.recipe,
        })
    }

    /// `frame` as [`render`](Self::render) makes it, before its JPEG: such
    /// as the live mosaic draws (P38).
    pub fn display(
        &self,
        frame: &Frame,
        request: &PreviewRequest,
        look: &Look,
        gpu: bool,
    ) -> Result<Displayed, PreviewError> {
        let meta = frame.meta();
        let view = frame.view();
        let format = view.format();
        let pattern = format.bayer();
        if !cpu::converts(format) || !matches!(format.channels(), 1 | 3) {
            return Err(PreviewError::Format(meta.format));
        }
        let (rect, roi) = region(meta, request.roi, if pattern.is_some() { 2 } else { 1 })?;
        let view = view.roi(rect).map_err(|_| PreviewError::Roi {
            roi,
            frame: meta.roi,
        })?;
        let (width, height) = fit(rect.width, rect.height, request.viewport);
        let superpixel = pattern.is_some() && 2 * width <= rect.width && 2 * height <= rect.height;
        let (wide, high) = match superpixel {
            true => (rect.width / 2, rect.height / 2),
            false => (rect.width, rect.height),
        };
        let region = Region {
            x: f64::from(rect.x),
            y: f64::from(rect.y),
            width: f64::from(rect.width),
            height: f64::from(rect.height),
            frame_width: f64::from(meta.width),
            frame_height: f64::from(meta.height),
        };
        let recipe = self.recipe();
        let steps = Steps {
            pattern,
            method: match superpixel {
                true => Demosaic::Superpixel,
                false => Demosaic::Bilinear,
            },
            bin: (wide / width).min(high / height).max(1),
            black: black(meta),
            look: *look,
        };
        self.compute.pools().install(Lane::Live, || {
            let coloured = self.colour(&view, &steps, gpu, recipe.as_ref(), region)?;
            let (pixels, shape, on_gpu, recipe) = coloured;
            Ok(Displayed {
                rgb: resize::resize(&pixels, shape, width, height),
                roi,
                width,
                height,
                gpu: on_gpu,
                recipe,
            })
        })
    }

    /// `frame`'s live statistics, as `request` asks, for `rendered`, its
    /// preview: in the live pool ([`crate::stats`]).
    pub fn measure(
        &self,
        frame: &Frame,
        request: &StatsRequest,
        rendered: &Rendered,
    ) -> Result<(LiveStats, Option<Mask>), PreviewError> {
        let size = (rendered.width, rendered.height);
        self.compute.pools().install(Lane::Live, || {
            stats::measure(frame, request, rendered.roi, size)
        })
    }

    /// Steps 2–4, with the live recipe if there is one, for the part of the
    /// frame `region` is.
    fn colour(
        &self,
        view: &FrameView<'_>,
        steps: &Steps,
        gpu: bool,
        recipe: Option<&LiveRecipe>,
        region: Region,
    ) -> Result<Coloured, PreviewError> {
        #[cfg(feature = "cuda")]
        if gpu && let Some(cuda) = self.compute.cuda() {
            let (pixels, shape, took) = on_gpu(cuda, view, steps, recipe, region)?;
            return Ok((pixels, shape, true, took));
        }
        let _ = gpu;
        let mut image = self.compute.to_f32(Lane::Live, view)?;
        if let Some(pattern) = steps.pattern {
            image = color::demosaic(&image, pattern, steps.method)?;
        }
        if steps.bin > 1 {
            image = cpu::downsample(&image, steps.bin)?;
        }
        if steps.black > 0.0 {
            color::subtract_black(&mut image, steps.black)?;
        }
        if let Some(recipe) = recipe {
            let (shown, took) = recipe.run(Value::image(image), region)?;
            return Ok((bytes(&shown), shown.shape(), false, Some(took)));
        }
        if image.channels() == 3 && steps.look.correction != ColourCorrection::default() {
            color::correct(&mut image, &steps.look.correction)?;
        }
        let pixels = color::to_display(&image, &steps.look.display)?;
        Ok((pixels, image.shape(), false, None))
    }
}

/// Display values from 0 to 1 as 8-bit samples.
fn bytes(image: &Image) -> Vec<u8> {
    let samples = image.data().par_iter();
    samples
        .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
        .collect()
}

/// [`Preview::colour`] on the GPU.
#[cfg(feature = "cuda")]
fn on_gpu(
    cuda: &oscope_compute::cuda::Cuda,
    view: &FrameView<'_>,
    steps: &Steps,
    recipe: Option<&LiveRecipe>,
    region: Region,
) -> Result<(Vec<u8>, Shape, Option<Duration>), PreviewError> {
    use oscope_imaging::color::gpu;
    let lane = Lane::Live;
    let mut image = cuda.to_f32(lane, view)?;
    if let Some(pattern) = steps.pattern {
        image = gpu::demosaic(cuda, lane, &image, pattern, steps.method)?;
    }
    if steps.bin > 1 {
        image = cuda.downsample(lane, &image, steps.bin)?;
    }
    if steps.black > 0.0 {
        gpu::subtract_black(cuda, lane, &mut image, steps.black)?;
    }
    if let Some(recipe) = recipe {
        let (shown, took) = recipe.run(Value::DeviceImage(Arc::new(image)), region)?;
        return Ok((bytes(&shown), shown.shape(), Some(took)));
    }
    if image.shape().channels == 3 && steps.look.correction != ColourCorrection::default() {
        gpu::correct(cuda, lane, &mut image, &steps.look.correction)?;
    }
    let pixels = gpu::to_display(cuda, lane, &image, &steps.look.display)?;
    Ok((pixels, image.shape(), None))
}

/// The frame's pixels that `roi` covers, clipped to the frame and widened to
/// whole cells of `cell` pixels; and the sensor's pixels they are.
pub(crate) fn region(
    meta: &FrameMeta,
    roi: Option<PixelRect>,
    cell: u32,
) -> Result<(PixelRect, PixelRect), PreviewError> {
    let binning = meta.binning.max(1);
    let (width, height) = (meta.width / cell * cell, meta.height / cell * cell);
    let rect = match roi {
        None => PixelRect::new(0, 0, width, height),
        Some(roi) => {
            // From the sensor's pixels to the frame's, outwards to whole cells.
            let span = |start: u32, length: u32, origin: u32, size: u32| {
                let first = start.saturating_sub(origin) / binning / cell * cell;
                let end = (start + length).saturating_sub(origin).div_ceil(binning);
                (first, (end.div_ceil(cell) * cell).min(size))
            };
            let (x0, x1) = span(roi.x, roi.width, meta.roi.x, width);
            let (y0, y1) = span(roi.y, roi.height, meta.roi.y, height);
            if roi.width == 0 || roi.height == 0 || x1 <= x0 || y1 <= y0 {
                return Err(PreviewError::Roi {
                    roi,
                    frame: meta.roi,
                });
            }
            PixelRect::new(x0, y0, x1 - x0, y1 - y0)
        }
    };
    let sensor = PixelRect::new(
        meta.roi.x + rect.x * binning,
        meta.roi.y + rect.y * binning,
        rect.width * binning,
        rect.height * binning,
    );
    Ok((rect, sensor))
}

/// The size `width` × `height` takes inside `viewport`: the same shape,
/// never larger, and at least 1 × 1.
fn fit(width: u32, height: u32, (wide, high): (u32, u32)) -> (u32, u32) {
    let scale = (f64::from(wide) / f64::from(width))
        .min(f64::from(high) / f64::from(height))
        .min(1.0);
    let size = |n: u32| ((f64::from(n) * scale).round() as u32).clamp(1, n);
    (size(width), size(height))
}

/// The frame's black level, on the 0–1 scale its samples are read to.
fn black(meta: &FrameMeta) -> f32 {
    let full = match meta.format.sample_bytes() {
        2 => 65_535.0,
        _ => 255.0,
    };
    ((meta.black_level / full) as f32).clamp(0.0, 0.99)
}
