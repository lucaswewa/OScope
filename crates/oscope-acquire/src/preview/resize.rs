//! The preview's last resize, in 8 bits: a box filter over the fraction
//! that binning by whole factors leaves, so every input pixel counts by the
//! area it covers.

use oscope_compute::Shape;
use rayon::prelude::*;

/// For each of `to` outputs, the first of the `from` inputs it covers, and
/// the share of the output each input covers: a box `from / to` wide.
fn spans(from: u32, to: u32) -> Vec<(usize, Vec<f32>)> {
    let step = f64::from(from) / f64::from(to);
    (0..to)
        .map(|o| {
            let (start, end) = (f64::from(o) * step, f64::from(o + 1) * step);
            let first = start.floor() as usize;
            let last = (end.ceil() as usize).min(from as usize);
            let weights = (first..last)
                .map(|i| ((i + 1) as f64).min(end) - (i as f64).max(start))
                .map(|covered| (covered / step) as f32)
                .collect();
            (first, weights)
        })
        .collect()
}

/// `pixels`, grey or RGB in `shape`, as RGB `width` × `height`: no larger
/// than `shape`. Grey becomes three equal channels.
pub(crate) fn resize(pixels: &[u8], shape: Shape, width: u32, height: u32) -> Vec<u8> {
    let channels = shape.channels;
    if channels == 3 && (shape.width, shape.height) == (width, height) {
        return pixels.to_vec();
    }
    let (columns, rows) = (spans(shape.width, width), spans(shape.height, height));
    let line = shape.row_len();
    let mut out = vec![0_u8; width as usize * height as usize * 3];
    out.par_chunks_mut(width as usize * 3)
        .zip(&rows)
        .for_each(|(row, (top, row_weights))| {
            let mut sums = vec![0.0_f32; width as usize * channels];
            for (dy, wy) in row_weights.iter().enumerate() {
                let source = &pixels[(top + dy) * line..][..line];
                for (sum, (left, column_weights)) in sums.chunks_exact_mut(channels).zip(&columns) {
                    for (dx, wx) in column_weights.iter().enumerate() {
                        let pixel = &source[(left + dx) * channels..][..channels];
                        for (s, &v) in sum.iter_mut().zip(pixel) {
                            *s += wy * wx * f32::from(v);
                        }
                    }
                }
            }
            for (out, sum) in row
                .as_chunks_mut::<3>()
                .0
                .iter_mut()
                .zip(sums.chunks_exact(channels))
            {
                for (c, sample) in out.iter_mut().enumerate() {
                    *sample = sum[c.min(channels - 1)].round().clamp(0.0, 255.0) as u8;
                }
            }
        });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_weights_cover_each_output_once() {
        for (from, to) in [(10, 10), (2736, 1920), (7, 3), (5, 1)] {
            let spans = spans(from, to);
            for (first, weights) in &spans {
                let total: f32 = weights.iter().sum();
                assert!((total - 1.0).abs() < 1e-4, "{from} → {to}: {total}");
                assert!(first + weights.len() <= from as usize);
            }
        }
    }

    #[test]
    fn a_ramp_keeps_its_mean_and_grey_becomes_rgb() {
        let shape = Shape::new(9, 1, 1);
        let ramp: Vec<u8> = (0..9).map(|i| i * 30).collect();
        let out = resize(&ramp, shape, 3, 1);
        assert_eq!(out, vec![30, 30, 30, 120, 120, 120, 210, 210, 210]);
    }
}
