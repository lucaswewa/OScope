//! The frames P34's procedures look at: a region of each, made grey and
//! binned for the focus measures; its linear samples for exposure; its
//! lightest cells' colours for white balance.

use std::time::Duration;

use oscope_acquire::FrameBus;
use oscope_compute::{Image, Shape, cpu};
use oscope_core::{BayerColour, Frame, FrameMeta, PixelFormat, Timestamp};

use super::{AutofocusError, failed};
use crate::calibration::Region;

/// How long each frame is waited for, as calibration's frames are.
const FRAME_WAIT: Duration = Duration::from_secs(30);
/// The binned region's longest side, in pixels.
const BINNED: u32 = 512;
/// A sample this share of full scale, or more, is saturated.
pub(super) const SATURATED: f64 = 0.98;

/// The first frame whose exposure starts at `after` or later.
pub(super) async fn frame_after(bus: &FrameBus, after: Timestamp) -> Result<Frame, AutofocusError> {
    tokio::time::timeout(FRAME_WAIT, bus.next_frame_after(after))
        .await
        .map_err(|_| failed("no frame came within 30 s: is the camera streaming?"))?
        .ok_or_else(|| failed("the camera's frames stopped"))
}

fn mosaic(format: PixelFormat) -> bool {
    matches!(
        format,
        PixelFormat::Bayer8 { .. } | PixelFormat::Bayer16 { .. }
    )
}

/// The black level of `meta`'s samples, in linear units.
fn black(meta: &FrameMeta) -> f32 {
    let full = if meta.format.sample_bytes() == 2 {
        65_535.0
    } else {
        255.0
    };
    (meta.black_level / full) as f32
}

/// A frame as linear samples, its region, and its black level.
struct Linear {
    image: Image,
    /// x, y, width, height.
    region: (u32, u32, u32, u32),
    black: f32,
    mosaic: bool,
    format: PixelFormat,
}

impl Linear {
    fn at(&self, x: u32, y: u32, c: usize) -> f32 {
        let (w, channels) = (self.image.width(), self.image.channels());
        self.image.data()[(y * w + x) as usize * channels + c] - self.black
    }

    /// Full scale, less the black level.
    fn full(&self) -> f64 {
        f64::from(1.0 - self.black)
    }
}

/// `frame` as linear samples, over `region` or its middle half each way:
/// within the frame, on whole 2 × 2 cells of a mosaic.
fn linear(frame: &Frame, region: Option<Region>) -> Result<Linear, AutofocusError> {
    let meta = frame.meta();
    let image = cpu::to_f32(&frame.view()).map_err(|e| failed(format!("a frame: {e}")))?;
    let mosaic = mosaic(meta.format);
    let (w, h) = (image.width(), image.height());
    let even = |v: u32| if mosaic { v & !1 } else { v };
    let (x, y, rw, rh) = match region {
        Some(r) => (r.x, r.y, r.width, r.height),
        None => (w / 4, h / 4, w / 2, h / 2),
    };
    let (x, y) = (even(x), even(y));
    let (rw, rh) = (
        even(rw.min(w.saturating_sub(x))),
        even(rh.min(h.saturating_sub(y))),
    );
    if rw < 6 || rh < 6 {
        return Err(failed(format!(
            "the region from ({x}, {y}) isn't inside the {w} × {h} frame, or is under 6 pixels"
        )));
    }
    Ok(Linear {
        image,
        region: (x, y, rw, rh),
        black: black(meta),
        mosaic,
        format: meta.format,
    })
}

/// The middle row of `region`, or of the frame's middle half, in the
/// frame's rows: when a rolling shutter exposed it.
pub(super) fn middle_row(meta: &FrameMeta, region: Option<Region>) -> u32 {
    match region {
        Some(r) => (r.y + r.height / 2).min(meta.height.saturating_sub(1)),
        None => meta.height / 2,
    }
}

/// A frame's region, grey, linear and binned until its longest side is at
/// most 512 pixels: a mosaic's 2 × 2 cells summed, a colour pixel's colours
/// averaged.
pub(super) fn prepare(frame: &Frame, region: Option<Region>) -> Result<Image, AutofocusError> {
    let linear = linear(frame, region)?;
    let (x0, y0, rw, rh) = linear.region;
    let channels = linear.image.channels();
    let (gw, gh) = if linear.mosaic {
        (rw / 2, rh / 2)
    } else {
        (rw, rh)
    };
    let grey = |gx: u32, gy: u32| match (linear.mosaic, channels) {
        (true, _) => {
            let (x, y) = (x0 + 2 * gx, y0 + 2 * gy);
            linear.at(x, y, 0)
                + linear.at(x + 1, y, 0)
                + linear.at(x, y + 1, 0)
                + linear.at(x + 1, y + 1, 0)
        }
        (false, 3..) => {
            let (x, y) = (x0 + gx, y0 + gy);
            (linear.at(x, y, 0) + linear.at(x, y, 1) + linear.at(x, y, 2)) / 3.0
        }
        (false, _) => linear.at(x0 + gx, y0 + gy, 0),
    };
    let k = gw.max(gh).div_ceil(BINNED).max(1);
    let (bw, bh) = (gw / k, gh / k);
    let mut data = Vec::with_capacity((bw * bh) as usize);
    for by in 0..bh {
        for bx in 0..bw {
            let mut sum = 0.0;
            for gy in by * k..(by + 1) * k {
                for gx in bx * k..(bx + 1) * k {
                    sum += grey(gx, gy);
                }
            }
            data.push(sum / (k * k) as f32);
        }
    }
    Image::from_vec(Shape::new(bw, bh, 1), data).map_err(|e| failed(e.to_string()))
}

/// The region's `percentile` (from 0 to 100) of its linear samples, as a
/// share of full scale: each site of a mosaic, each colour of a pixel.
pub(super) fn percentile(
    frame: &Frame,
    region: Option<Region>,
    percentile: f64,
) -> Result<f64, AutofocusError> {
    let linear = linear(frame, region)?;
    let (x0, y0, rw, rh) = linear.region;
    let channels = linear.image.channels();
    let mut values = Vec::with_capacity((rw * rh) as usize * channels);
    for y in y0..y0 + rh {
        for x in x0..x0 + rw {
            for c in 0..channels {
                values.push(linear.at(x, y, c));
            }
        }
    }
    let k = ((percentile / 100.0) * (values.len() - 1) as f64).round() as usize;
    let (_, value, _) = values.select_nth_unstable_by(k, f32::total_cmp);
    Ok(f64::from(*value) / linear.full())
}

/// The light's white in the region: the mean red, green and blue of its
/// cells (a mosaic's 2 × 2, or a colour pixel) whose grey lies between the
/// region's 95th percentile and saturation, as shares of full scale; their
/// mean grey; and how many there are. `None` for a grey frame.
pub(super) fn white(
    frame: &Frame,
    region: Option<Region>,
) -> Result<Option<([f64; 3], f64, usize)>, AutofocusError> {
    let linear = linear(frame, region)?;
    let (x0, y0, rw, rh) = linear.region;
    let full = linear.full();
    let cells: Vec<[f64; 3]> = match (linear.format, linear.image.channels()) {
        (PixelFormat::Bayer8 { pattern } | PixelFormat::Bayer16 { pattern, .. }, _) => {
            let mut cells = Vec::with_capacity((rw * rh / 4) as usize);
            for y in (y0..y0 + rh).step_by(2) {
                for x in (x0..x0 + rw).step_by(2) {
                    let mut rgb = [0.0; 3];
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let c = match pattern.colour_at((x + dx) % 2, (y + dy) % 2) {
                            BayerColour::Red => 0,
                            BayerColour::Green => 1,
                            BayerColour::Blue => 2,
                        };
                        // The two greens are averaged.
                        let share = if c == 1 { 0.5 } else { 1.0 };
                        rgb[c] += share * f64::from(linear.at(x + dx, y + dy, 0)) / full;
                    }
                    cells.push(rgb);
                }
            }
            cells
        }
        (_, 3..) => (y0..y0 + rh)
            .flat_map(|y| (x0..x0 + rw).map(move |x| (x, y)))
            .map(|(x, y)| [0, 1, 2].map(|c| f64::from(linear.at(x, y, c)) / full))
            .collect(),
        _ => return Ok(None),
    };
    let grey = |c: &[f64; 3]| (c[0] + 2.0 * c[1] + c[2]) / 4.0;
    let mut greys: Vec<f64> = cells.iter().map(grey).collect();
    let k = ((greys.len() - 1) as f64 * 0.95).round() as usize;
    let (_, threshold, _) = greys.select_nth_unstable_by(k, f64::total_cmp);
    let threshold = *threshold;
    let lightest: Vec<&[f64; 3]> = cells
        .iter()
        .filter(|c| grey(c) >= threshold && c.iter().all(|v| *v < SATURATED))
        .collect();
    let n = lightest.len();
    if n == 0 {
        return Ok(Some(([0.0; 3], 0.0, 0)));
    }
    let mean = [0, 1, 2].map(|i| lightest.iter().map(|c| c[i]).sum::<f64>() / n as f64);
    Ok(Some((mean, grey(&mean), n)))
}
