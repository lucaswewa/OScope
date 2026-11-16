//! Depth from focus (P42): the parabola's vertex; heights within half a
//! step on a textured surface whose planes are blurred by their distance
//! from it, little confidence over featureless glass; and the all-in-focus
//! image nearer the sharp texture than any plane.

use oscope_compute::{Image, Shape, filters, focus};
use oscope_volume::{AllInFocus, FocusOptions, Peaks, Planes, all_in_focus, depth_from_focus};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

const PLANES: Planes = Planes {
    z0_um: 0.0,
    step_um: 0.5,
};

#[test]
fn the_parabola_s_vertex_is_the_height_and_a_peak_at_an_end_has_no_confidence() {
    // Three pixels over nine planes: measures on a parabola peaking at
    // plane 4.3; falling from the first; rising to the last.
    let (z0, step) = (100.0, 0.5);
    let mut peaks = Peaks::new(3, 1);
    for k in 0..9 {
        let parabola = 30.0 - (k as f32 - 4.3).powi(2);
        let measures = vec![parabola, 9.0 - k as f32, 1.0 + k as f32];
        peaks
            .push(&Image::from_vec(Shape::new(3, 1, 1), measures).unwrap())
            .unwrap();
    }
    let depth = peaks
        .finish(Planes {
            z0_um: z0,
            step_um: step,
        })
        .unwrap();
    let (h, c) = (depth.heights.data(), depth.confidence.data());
    assert!((h[0] - (z0 + 4.3 * step) as f32).abs() < 1e-4, "{h:?}");
    // The peak over the curve's mean: 30 − 0.09 against 30 less the mean of
    // (k − 4.3)², 60 / 9 + 0.09.
    let (best, mean) = (29.91_f64, 30.0 - 60.0 / 9.0 - 0.09);
    assert!(
        (f64::from(c[0]) - (best - mean) / best).abs() < 1e-5,
        "{c:?}"
    );
    assert_eq!((h[1], c[1]), (z0 as f32, 0.0));
    assert_eq!((h[2], c[2]), ((z0 + 8.0 * step) as f32, 0.0));
    // A tie keeps the lower plane: 1, 3, 1, 3, 1 peaks at plane 1, not 3.
    let mut tied = Peaks::new(1, 1);
    for f in [1.0, 3.0, 1.0, 3.0, 1.0] {
        tied.push(&Image::from_vec(Shape::new(1, 1, 1), vec![f]).unwrap())
            .unwrap();
    }
    let depth = tied.finish(PLANES).unwrap();
    assert_eq!(depth.heights.data()[0], 0.5);
    // Refused: planes of another size, fewer than 3, and no step.
    let mut few = Peaks::new(2, 2);
    assert!(few.push(&Image::zeros(Shape::new(2, 3, 1))).is_err());
    few.push(&Image::zeros(Shape::new(2, 2, 1))).unwrap();
    few.push(&Image::zeros(Shape::new(2, 2, 1))).unwrap();
    assert!(few.finish(PLANES).is_err());
    let flat = Planes {
        z0_um: 0.0,
        step_um: 0.0,
    };
    assert!(peaks.finish(flat).is_err());
}

/// The test's surface: its texture, sharp, its height in µm at each pixel,
/// and its planes, each pixel blurred by its distance from focus, 2.5 pixels
/// a µm beyond a floor of 0.3, with a little noise. The right quarter is
/// featureless glass.
struct Surface {
    texture: Image,
    heights: Vec<f32>,
    stack: Vec<Image>,
    width: usize,
    glass: usize,
}

fn surface() -> Surface {
    let (w, h) = (160_usize, 120_usize);
    let glass = 120;
    let mut random = StdRng::seed_from_u64(42);
    let noise: Vec<f32> = (0..w * h).map(|_| random.random_range(0.0..1.0)).collect();
    let rough = filters::gaussian(
        &Image::from_vec(Shape::new(w as u32, h as u32, 1), noise).unwrap(),
        1.0,
    )
    .unwrap();
    let texture: Vec<f32> = rough
        .data()
        .iter()
        .enumerate()
        .map(|(i, v)| {
            if i % w < glass {
                0.5 + 2.0 * (v - 0.5)
            } else {
                0.5
            }
        })
        .collect();
    let texture = Image::from_vec(Shape::new(w as u32, h as u32, 1), texture).unwrap();
    let heights: Vec<f32> = (0..w * h)
        .map(|i| {
            let (x, y) = ((i % w) as f32, (i / w) as f32);
            2.5 + 1.2 * (x / 25.0).sin() * (y / 20.0).cos()
        })
        .collect();
    // The texture blurred at every 0.25 pixels of σ, each pixel then between
    // the two nearest.
    let levels: Vec<Image> = (0..52)
        .map(|k| filters::gaussian(&texture, 0.3 + 0.25 * k as f32).unwrap())
        .collect();
    let stack = (0..11)
        .map(|k| {
            let z = PLANES.z0_um as f32 + k as f32 * PLANES.step_um as f32;
            let data = (0..w * h)
                .map(|i| {
                    let sigma = 0.3 + 2.5 * (z - heights[i]).abs();
                    let u = ((sigma - 0.3) / 0.25).min(50.999);
                    let (j, t) = (u.floor() as usize, u.fract());
                    let blurred = (1.0 - t) * levels[j].data()[i] + t * levels[j + 1].data()[i];
                    blurred + random.random_range(-0.01..0.01)
                })
                .collect();
            Image::from_vec(texture.shape(), data).unwrap()
        })
        .collect();
    Surface {
        texture,
        heights,
        stack,
        width: w,
        glass,
    }
}

/// The median of `values`.
fn median(mut values: Vec<f32>) -> f32 {
    values.sort_by(f32::total_cmp);
    values[values.len() / 2]
}

#[test]
fn a_textured_surface_s_heights_are_within_half_a_step_and_glass_has_little_confidence() {
    let s = surface();
    let options = FocusOptions::default();
    let measure = |plane: &Image| focus::sml(plane, options.step, options.radius);
    let depth = depth_from_focus(&s.stack, PLANES, &options, measure).unwrap();
    // The heights' median is the filter's, over those found without it.
    let unfiltered = FocusOptions {
        median: 0,
        ..options
    };
    let raw = depth_from_focus(&s.stack, PLANES, &unfiltered, measure).unwrap();
    assert_eq!(
        depth.heights,
        filters::median(&raw.heights, options.median).unwrap()
    );
    assert_ne!(depth.heights, raw.heights);
    let (heights, confidence) = (depth.heights.data(), depth.confidence.data());
    // Textured: left of the glass by more than the window, and the median.
    let textured: Vec<usize> = (0..heights.len())
        .filter(|i| i % s.width < s.glass - 4)
        .collect();
    let glass: Vec<usize> = (0..heights.len())
        .filter(|i| i % s.width > s.glass + 4)
        .collect();
    let sure: Vec<usize> = textured
        .iter()
        .copied()
        .filter(|&i| confidence[i] >= 0.5)
        .collect();
    assert!(
        sure.len() * 10 >= textured.len() * 9,
        "{} of {} sure",
        sure.len(),
        textured.len()
    );
    let squares: f64 = sure
        .iter()
        .map(|&i| f64::from(heights[i] - s.heights[i]).powi(2))
        .sum();
    let rms = (squares / sure.len() as f64).sqrt();
    assert!(rms < PLANES.step_um / 2.0, "rms {rms:.3} µm");
    let on = |pixels: &[usize]| median(pixels.iter().map(|&i| confidence[i]).collect());
    let (on_texture, on_glass) = (on(&textured), on(&glass));
    assert!(
        on_texture > 0.6 && on_glass < 0.4,
        "{on_texture} {on_glass}"
    );

    // The all-in-focus image: nearer the sharp texture than any one plane.
    let fused = all_in_focus(&s.stack, &depth.heights, PLANES).unwrap();
    let off = |image: &Image| {
        let sum: f64 = textured
            .iter()
            .map(|&i| f64::from(image.data()[i] - s.texture.data()[i]).powi(2))
            .sum();
        (sum / textured.len() as f64).sqrt()
    };
    let best_plane = s.stack.iter().map(off).fold(f64::MAX, f64::min);
    eprintln!(
        "{} of {} textured pixels sure, {rms:.3} µm rms; confidence {on_texture:.2} on texture, {on_glass:.2} on glass; all in focus {:.4} off, the best plane {best_plane:.4}",
        sure.len(),
        textured.len(),
        off(&fused)
    );
    assert!(
        off(&fused) < 0.7 * best_plane,
        "{} against {best_plane}",
        off(&fused)
    );
}

#[test]
fn the_all_in_focus_image_blends_the_planes_either_side_of_each_height() {
    // Two pixels of three channels, over three planes 1 µm apart, from 10:
    // the first at 10.25, a quarter of the way to the second plane; the
    // second at the top.
    let planes = Planes {
        z0_um: 10.0,
        step_um: 1.0,
    };
    let heights = Image::from_vec(Shape::new(2, 1, 1), vec![10.25, 13.0]).unwrap();
    let stack: Vec<Image> = (0..3)
        .map(|k| Image::from_vec(Shape::new(2, 1, 3), vec![k as f32; 6]).unwrap())
        .collect();
    let fused = all_in_focus(&stack, &heights, planes).unwrap();
    assert_eq!(fused.data(), &[0.25, 0.25, 0.25, 2.0, 2.0, 2.0]);
    // A plane too many, or too few, is refused.
    let mut partial = AllInFocus::new(&heights, planes, 3, 3).unwrap();
    partial.push(&stack[0]).unwrap();
    assert!(partial.clone().finish().is_err());
    assert!(partial.push(&Image::zeros(Shape::new(2, 1, 1))).is_err());
    let mut full = AllInFocus::new(&heights, planes, 1, 3).unwrap();
    full.push(&stack[0]).unwrap();
    assert!(full.push(&stack[1]).is_err());
}
